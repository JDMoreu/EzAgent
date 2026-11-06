# Product

<!-- impeccable:product-schema 1 -->

## Platform

desktop (Windows)

## Stack

Python voice messenger with a Win32 layered notch. VAD and turn segmentation run
locally; whisper.cpp provides STT, Kokoro provides TTS, and the LLM is selected
from any OpenAI-compatible provider. There is no Tauri/React app, dashboard,
call panel, or secondary settings window.

## Users

A single Spanish-speaking power user on their own Windows 11 desktop with a GPU
and headphones, working alongside Herdr. They want hands-free spoken control of
their agents, with a compact always-available notch and a menu for adjusting
audio, voice and conversation behavior.

## Product Purpose

EzAgent is a private, low-latency voice messenger. The notch is the entire
interface: it shows attention through two eyes, listens, answers and exposes
its settings only when expanded. Success is a conversation that starts
immediately and a notch that stays out of the way.

## Positioning

The listening pipeline runs locally — VAD, whisper.cpp and Kokoro — while the
LLM can be OpenCode, OpenAI, Groq, Claude, Gemini or any OpenAI-compatible
endpoint. The notch makes that hybrid pipeline visible without a dashboard.

## Operating Context

Windows 11, an always-on-top layered notch, a global hotkey, an external
whisper-server, and an optional Herdr app for agent orchestration. The notch is
dragged like a floating pill and attaches to the top or bottom edge. Provider
keys and models can be configured through `.env`, `llm.json` or the notch menu;
the notch only takes keyboard focus while an inline text field is being edited.

## Capabilities and Constraints

- Start/stop the voice call; the app starts listening when opened.
- Mute the microphone; the muted icon is red and crossed.
- Open the settings menu in the same expanding notch: microphone, VAD,
  silence, voice, TTS, Whisper/STT, LLM/API, interruption, optional confirmations
  and voice preview. Long content scrolls; all labels are Spanish.
- Edit provider, model, endpoint and API key inline; save providers and switch
  between saved profiles without leaving the notch.
- Drag the notch anywhere; attach it to the top or bottom work-area edge.
- Configure the LLM provider/model through the menu, environment variables or
  `llm.json`.
- No secondary configuration window, no dashboard, no analytics, no accounts.
- Spanish UI; factual copy; no invented metrics or capabilities.

## Brand Commitments

Name: EzAgent. Voice: direct, calm, technical without jargon, Spanish from
Spain. The interface is an instrument, not a marketing surface. The identity is
«the instrument that looks back»: graphite body, two capsule eyes as the only
symbol, one mint signal for attention, amber for processing or pause, red for
failure. The notch is the product; there is no desktop panel behind it.

## Evidence on Hand

- `README.md` documents the notch, installation and provider configuration.
- `docs/llm-providers.md` documents provider selection.
- `DESIGN.md` is the visual contract for the notch and its menu.
- `assets/icon.svg` is the eyes-in-rounded-square mark.
- No customer logos, testimonials, pricing, or invented benchmarks.

## Product Principles

1. The notch is the product; every control lives in it or its menu.
2. State is never ambiguous: idle, listening, thinking, speaking, notice,
   muted, error or hidden.
3. Settings are explicit, reversible, and saved only when requested.
4. Local-first trust is visible in language and behavior.
5. Every action is one gesture away; no dashboard detour.
