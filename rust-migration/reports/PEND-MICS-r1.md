# PEND-MICS — r1

## Resultado
- Brief/tarea: PEND-MICS (`--list-mics` y el índice global de PortAudio con
  las cuatro APIs de host: MME, DirectSound, WASAPI y WDM-KS)
- Estado: **completado**. `--list-mics` imprime exactamente las mismas 14
  líneas que el Python y `cli_parity` compara la lista completa. Queda una
  limitación documentada en la traducción índice → cpal de cuatro índices de
  WDM-KS que WASAPI no expone como endpoint activo (ver «Pendiente»).

## Cambios
- `rust/crates/ez-audio/src/capture/portaudio.rs`: ahora es el orquestador de
  las cuatro APIs (`mme`, `dsound`, `wasapi`, `wdmks`), en el orden de
  PortAudio, con el índice global calculado al añadir cada dispositivo y
  `names_match`/`wide_name` compartidos. El bloque de MME se movió a
  `portaudio/mme.rs` sin cambiar su lógica.
- `rust/crates/ez-audio/src/capture/portaudio/mme.rs`: el bloque de MME que ya
  existía (mapeadores, entradas y salidas, nombres y `DetectDefaultSampleRate`),
  movido para dejar sitio a las demás APIs.
- `rust/crates/ez-audio/src/capture/portaudio/dsound.rs`: bloque de
  DirectSound. Enumeración de capturas con `DirectSoundCaptureEnumerateW` y
  después de salidas con `DirectSoundEnumerateW`; `DirectSoundCaptureCreate` +
  `DSCCAPS` (con `dwSize`, que la envoltura de `windows` no rellena), descarte
  de `DSCAPS_EMULDRIVER`, canales del endpoint (KS/cpal) para las capturas y
  tasa desde los bits `WAVE_FORMAT_*` de `dwFormats`; en las salidas, tasa por
  el rango `dwMin/MaxSecondarySampleRate`.
- `rust/crates/ez-audio/src/capture/portaudio/wasapi.rs`: bloque de WASAPI con
  `IMMDeviceEnumerator::EnumAudioEndpoints(eAll, DEVICE_STATE_ACTIVE)` (primero
  reproducción y después captura), nombre de `PKEY_Device_FriendlyName`
  (recortado a 127 bytes) y tasa/canales del formato de mezcla
  (`IAudioClient::GetMixFormat`). Un endpoint que falla deja su hueco con
  nombre vacío y 0 canales, como `FillInactiveDeviceInfo`.
- `rust/crates/ez-audio/src/capture/portaudio/wdmks.rs` y
  `portaudio/wdmks/{ks,topology}.rs`: bloque de WDM-KS. Filtros KS de
  `KSCATEGORY_AUDIO` con alias de reproducción o captura por SetupAPI (orden del
  registro), `FriendlyName` del registro del interfaz, pines de streaming con
  `KSPROPERTY_PIN_COMMUNICATION/DATAFLOW/INTERFACES/MEDIUMS/DATARANGES`, nombre
  del pin vía el filtro de topología (`KSPROPERTY_PIN_PHYSICALCONNECTION`) y
  sufijo ` (filtro)`. Los pines de captura usan `KSPROPERTY_PIN_NAME` y los de
  reproducción la categoría del terminal (PortAudio compila
  `PA_WDMKS_USE_CATEGORY_FOR_PIN_NAMES=1`); se portan los multiplexores, el
  sufijo ` n` para nombres repetidos y la tabla de terminales USB.
- `rust/crates/ez-audio/src/capture/choice.rs`: solo documentación de
  `resolve_in` (vale para cualquier API; sin endpoint de cpal cae a la elección
  automática). Sin cambios de lógica.
- `rust/crates/ez-app/tests/cli_parity.rs`: `compare_list_mics` compara ahora
  la lista completa, línea a línea, sin recortar la salida del Python.
- `rust/Cargo.toml`: features de `windows` que hacían falta (misma versión y
  misma dependencia del workspace; ninguna dependencia nueva):
  `Win32_Devices_DeviceAndDriverInstallation`, `Win32_Media_Audio_DirectSound`,
  `Win32_Media_KernelStreaming`, `Win32_System_Com_StructuredStorage`,
  `Win32_System_IO`, `Win32_System_Variant`, `Win32_UI_Shell_PropertiesSystem`.

## Decisiones
- `// DECISION` en `wdmks.rs` (`open_ks`): se abre el filtro sin
  `FILE_FLAG_OVERLAPPED`. PortAudio lo usa con IOCTL síncronos y `lpOverlapped`
  nulo; aquí se abre síncrono para no depender de ese caso límite.
- `// DECISION` en `dsound.rs`: no se consulta `GetSpeakerConfig`; los canales
  de una salida no los usa nadie (no son micrófonos ni cambian el índice
  global). Los canales de las capturas sí se refinan con el endpoint KS/cpal,
  que es lo que necesita `--list-mics` para el TA82 mono (índice 9).
- `// DECISION` en `wdmks/topology.rs`: no se pide
  `KSPROPERTY_PIN_CINSTANCES`; PortAudio lo exige para abrir streams, pero la
  enumeración no lo usa.
- `// DECISION` implícita en `dsound.rs`: el dispositivo primario de
  DirectSound (GUID nulo) se marca como `mapper`, igual que el mapeador de MME,
  para que resuelva al micrófono por omisión del sistema.
- Los canales de captura de DirectSound se toman del endpoint de cpal con el
  mismo nombre (lo que PortAudio le pregunta al filtro KS con `pnpInterface`).
  Evita el `IKsPropertySet`/`DSPROPERTY_DIRECTSOUNDDEVICE_ENUMERATE_W`, que
  pediría más interfaces COM, y da las mismas cifras en esta máquina.
- `resolve_in` sigue siendo por nombre: los índices de MME, DirectSound, WASAPI
  y los WDM-KS que también existen en WASAPI se traducen al dispositivo de cpal
  correcto.

## Verificación
Desde `rust/`:

- `cargo fmt --all --check`: ok.
- `cargo clippy -p ez-audio -p ez-app --all-targets -- -D warnings`: ok.
- `cargo test -p ez-audio -p ez-app`: ok
  (107 + 1 + 2 + 1 + 1 + 1 de ez-audio; 320 + 1 + 1 de ez-app; `cli_parity`
  con la lista completa en verde).
- Comparación real `--list-mics` contra el Python del `.venv` (mismo `argv[0]`
  y `PYTHONIOENCODING=utf-8`): **14 líneas idénticas byte a byte**
  (`cmp` sin diferencias). Índices 0, 1, 2, 8, 9, 10, 20, 21, 22, 23, 24, 28,
  31 y 33, con los mismos nombres, tasas y canales.
- Lista completa de dispositivos (34, salidas incluidas): coincide con
  `get_device_info_by_index` del Python en los índices 0–33 (orden, nombre,
  tasa y sentido). Las únicas diferencias están en canales de **salidas** que
  no imprime `--list-mics` (MME/DS dan 2 donde PyAudio da 8 para Steam), porque
  PortAudio refina las salidas por KS y aquí no se usan.
- Elección real con `choose_mic`: índice 4 (una salida de MME, el valor del
  `voice.json` del usuario) → Webcam AC410 (índice 2, cpal 5); índices 9, 20 y
  28 (TA82 por DirectSound, WASAPI y WDM-KS) → TA82 (cpal 4); índice 22
  (Realtek de WDM-KS) → elección automática Webcam, igual que un índice
  inservible en Python.

## Fixtures y pruebas
- Fixtures consumidos: ninguno (el brief no define fixtures para esta tarea).
- Filas nuevas de `parity/test-map.csv`: ninguna.
- Pruebas nuevas: `capture::portaudio` 2 (índice global y nombres), `mme` 4
  (las que ya existían, movidas), `dsound` 2 (tasas de captura y de salida),
  `wasapi` 2 (recorte de nombres y clave de nombre), `wdmks` 2 (composición de
  nombres y filtro sin nombre) y `wdmks/{ks,topology}` 6 (identificadores, GUID,
  lectura acotada, recorrido de captura/salida y multiplexor).

## Desviaciones del brief
- `rust/crates/ez-audio/src/capture.rs` conserva un comentario que dice
  «Enumera el bloque de MME» sobre `list_portaudio_devices`; no se tocó porque
  no está en la lista de archivos del brief. El comportamiento ya es el de las
  cuatro APIs.
- Se añadieron features de `windows` en `rust/Cargo.toml` (el brief lo permite
  expresamente); no se añadió ninguna dependencia.

## Pendiente o riesgos
- **Cuatro índices de WDM-KS no se pueden abrir con cpal en esta máquina**:
  22 (Línea de entrada Realtek), 23 (Micrófono Realtek), 24 (Mezcla estéreo
  Realtek) y 31 (Input de Steam Streaming Speakers) existen como pines KS pero
  no como endpoints WASAPI activos, que es lo único que enumera cpal. Se
  traducen a `None` y caen a la elección automática (hoy el Webcam AC410). No
  es un fallo de la enumeración: la lista y los índices son idénticos a
  PyAudio; es que Rust no tiene backend WDM-KS para abrirlos. Si algún día se
  añade, `resolve_in` es el único punto que hay que ampliar.
- Los canales de las **salidas** de MME y DirectSound no son idénticos a
  PyAudio (PortAudio los refina por KS; aquí no se usan). No afectan a
  `--list-mics`, a la elección de micrófono ni a `voice.json`.
- WDM-KS abre cada filtro y su topología durante la enumeración (como
  PortAudio). `list_portaudio_devices` se llama al crear `MicCapture` y al
  abrir el menú; el coste medido es de milisegundos y no bloquea hilos de audio
  ni UI más que la enumeración anterior de MME.
- Los nombres de categoría de WDM-KS son los ingleses de PortAudio
  (`Headphones`, `Speakers`, `Output`…): es lo que imprime PyAudio en esta
  máquina y forma parte de la paridad; no se traducen.
