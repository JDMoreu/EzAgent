//! Salida real con `cpal`: un stream por frase, como PyAudio.
//!
//! Aquí vive lo único que toca hardware. `finish` es el momento en que el audio
//! suena de verdad, así que es ahí donde se publica la envolvente que usa el
//! detector de barge-in.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ez_audio::barge_in::PlaybackReference;

use super::{pcm_rms, AudioSink, AudioStream, CHUNK_SAMPLES};

/// `pyaudio.PyAudio` + `open(output_device_index=...)` en cpal.
#[derive(Debug, Clone, Copy, Default)]
pub struct CpalSink;

impl CpalSink {
    fn device(&self, index: Option<usize>) -> Result<cpal::Device, String> {
        let host = cpal::default_host();
        match index {
            Some(index) => host
                .devices()
                .map_err(|error| error.to_string())?
                .nth(index)
                .ok_or_else(|| format!("No hay ningún dispositivo de salida con índice {index}.")),
            None => host
                .default_output_device()
                .ok_or_else(|| "No hay ningún dispositivo de salida disponible.".to_string()),
        }
    }
}

impl AudioSink for CpalSink {
    fn open(&self, rate: u32, device_index: Option<usize>) -> Result<Box<dyn AudioStream>, String> {
        let device = self.device(device_index)?;
        let (config, format) = output_config(&device, rate)?;
        let device_rate = config.sample_rate;
        let channels = usize::from(config.channels);
        let (sender, receiver) = crossbeam_channel::bounded::<i16>(4_096);
        let pending = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&pending);
        let stream = device
            .build_output_stream_raw(
                config,
                format,
                move |data, _| fill_output(data, format, channels, &receiver, &counter),
                |error| tracing::warn!("Audio: {error}"),
                Some(Duration::from_secs(2)),
            )
            .map_err(|error| error.to_string())?;
        stream.play().map_err(|error| error.to_string())?;
        Ok(Box::new(CpalStream {
            source_rate: rate,
            device_rate,
            buffered: Vec::new(),
            sender: Some(sender),
            pending,
            stream: Some(stream),
            played: false,
        }))
    }
}

/// Output configuration: the phrase rate if the device supports it, otherwise
/// the default one (the samples are resampled before playing).
fn output_config(
    device: &cpal::Device,
    rate: u32,
) -> Result<(cpal::StreamConfig, cpal::SampleFormat), String> {
    if let Ok(ranges) = device.supported_output_configs() {
        for range in ranges {
            if range.contains_rate(rate) && supported_format(range.sample_format()) {
                return Ok((range.with_sample_rate(rate).config(), range.sample_format()));
            }
        }
    }
    let default = device
        .default_output_config()
        .map_err(|error| error.to_string())?;
    if !supported_format(default.sample_format()) {
        return Err(format!(
            "Formato de salida no soportado: {:?}",
            default.sample_format()
        ));
    }
    Ok((default.config(), default.sample_format()))
}

fn supported_format(format: cpal::SampleFormat) -> bool {
    matches!(
        format,
        cpal::SampleFormat::F32
            | cpal::SampleFormat::I16
            | cpal::SampleFormat::I32
            | cpal::SampleFormat::U16
    )
}

struct CpalStream {
    source_rate: u32,
    device_rate: u32,
    buffered: Vec<i16>,
    sender: Option<crossbeam_channel::Sender<i16>>,
    pending: Arc<AtomicUsize>,
    stream: Option<cpal::Stream>,
    played: bool,
}

impl AudioStream for CpalStream {
    fn write(&mut self, samples: &[i16]) -> Result<(), String> {
        // shortcut: `write` acumula y `finish` reproduce; el HUD recibe los
        // niveles en ráfaga antes del sonido (PyAudio los daba en tiempo real).
        // Medir en hardware si hace falta; entonces el sink debe enviar aquí.
        self.buffered.extend_from_slice(samples);
        Ok(())
    }

    fn finish(
        &mut self,
        stop: &AtomicBool,
        reference: Option<&PlaybackReference>,
    ) -> Result<bool, String> {
        if self.played {
            return Ok(true);
        }
        self.played = true;
        let samples = resample_pcm(&self.buffered, self.source_rate, self.device_rate);
        let Some(sender) = self.sender.take() else {
            return Ok(false);
        };
        for chunk in samples.chunks(CHUNK_SAMPLES) {
            // La referencia va con retardo de un buffer (~80 ms), cubierto por
            // la ventana de eco del detector.
            if let Some(reference) = reference {
                reference.publish(pcm_rms(chunk));
            }
            for sample in chunk {
                loop {
                    if stop.load(Ordering::Relaxed) {
                        return Ok(false);
                    }
                    self.pending.fetch_add(1, Ordering::Relaxed);
                    match sender.send_timeout(*sample, Duration::from_millis(50)) {
                        Ok(()) => break,
                        Err(crossbeam_channel::SendTimeoutError::Timeout(_)) => {
                            self.pending.fetch_sub(1, Ordering::Relaxed);
                        }
                        Err(crossbeam_channel::SendTimeoutError::Disconnected(_)) => {
                            self.pending.fetch_sub(1, Ordering::Relaxed);
                            return Ok(false);
                        }
                    }
                }
            }
        }
        while self.pending.load(Ordering::Relaxed) > 0 {
            if stop.load(Ordering::Relaxed) {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        // Deja que el dispositivo renderice el último buffer antes de cerrar.
        std::thread::sleep(Duration::from_millis(50));
        self.stream = None;
        Ok(true)
    }
}

impl Drop for CpalStream {
    fn drop(&mut self) {
        self.sender = None;
        self.stream = None;
    }
}

/// Remuestreo lineal solo si el dispositivo no acepta la frecuencia de Kokoro.
fn resample_pcm(samples: &[i16], source_rate: u32, device_rate: u32) -> Vec<i16> {
    if source_rate == device_rate || source_rate == 0 || device_rate == 0 || samples.is_empty() {
        return samples.to_vec();
    }
    let source: Vec<f32> = samples
        .iter()
        .map(|sample| f32::from(*sample) / 32768.0)
        .collect();
    let Ok(resampled) =
        ez_audio::resample::resample_linear(&source, source_rate as i32, device_rate as i32)
    else {
        return samples.to_vec();
    };
    resampled
        .into_iter()
        .map(|value| (value as f32 * 32768.0).clamp(-32768.0, 32767.0) as i16)
        .collect()
}

fn fill_output(
    data: &mut cpal::Data,
    format: cpal::SampleFormat,
    channels: usize,
    receiver: &crossbeam_channel::Receiver<i16>,
    pending: &AtomicUsize,
) {
    match format {
        cpal::SampleFormat::F32 => write_frames(
            data.as_slice_mut::<f32>().unwrap_or_default(),
            channels,
            receiver,
            pending,
            |sample| f32::from(sample) / 32768.0,
        ),
        cpal::SampleFormat::I16 => write_frames(
            data.as_slice_mut::<i16>().unwrap_or_default(),
            channels,
            receiver,
            pending,
            |sample| sample,
        ),
        cpal::SampleFormat::I32 => write_frames(
            data.as_slice_mut::<i32>().unwrap_or_default(),
            channels,
            receiver,
            pending,
            |sample| i32::from(sample) << 16,
        ),
        cpal::SampleFormat::U16 => write_frames(
            data.as_slice_mut::<u16>().unwrap_or_default(),
            channels,
            receiver,
            pending,
            |sample| (i32::from(sample) + 32_768) as u16,
        ),
        _ => {}
    }
}

/// Duplica cada muestra mono en todos los canales del dispositivo.
fn write_frames<T: Copy>(
    output: &mut [T],
    channels: usize,
    receiver: &crossbeam_channel::Receiver<i16>,
    pending: &AtomicUsize,
    convert: impl Fn(i16) -> T,
) {
    if channels == 0 {
        return;
    }
    for frame in output.chunks_mut(channels) {
        let next = receiver.try_recv().ok();
        if next.is_some() {
            pending.fetch_sub(1, Ordering::Relaxed);
        }
        let value = convert(next.unwrap_or(0));
        for slot in frame {
            *slot = value;
        }
    }
}
