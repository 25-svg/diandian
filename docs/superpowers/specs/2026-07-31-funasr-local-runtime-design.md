# Local FunASR Runtime Integration

**Date:** 2026-07-31  
**Status:** Approved for planning  
**Scope:** Offline ASR provider selection and distributable FunASR runtime for the desktop app.

## Goal

Make local FunASR the preferred ASR engine for Chinese livestream recordings. Keep all recognition local, do not select or fall back to Volcengine, and automatically use the existing local Whisper model when FunASR cannot start or fails.

## Current state

- The saved setting is `subtitle_generator_type = "whisper"`.
- The local Whisper `small q5_1` model is present and usable.
- No distributable `funasr-service.exe` or development `.funasr-venv` is currently present.
- The backend already has a `funasr` provider path and a Whisper fallback, but the Settings UI does not expose FunASR as an explicit option.

## Approaches considered

1. **Bundle the official Windows offline FunASR Runtime and Chinese model.** Recommended. The app owns a known executable and model layout, works offline, and does not require the user to install Python.
2. Use a Python virtual environment in development. This is useful for development only but is not reliable for end users.
3. Call a remote FunASR service. This adds network and operational dependencies, contrary to the offline requirement.

## Design

### Runtime packaging

Add an app resource directory containing the official Windows CPU FunASR runtime and its required Chinese offline model assets. At startup, resolve the bundled runtime from the installed app resource directory; retain the existing environment-variable and development-venv lookup only for developer overrides.

The packaging/build script must verify that the service executable and required model assets exist before producing a distributable. It must record the upstream source URL and pinned release/version in a repository manifest. No credentials are stored with the runtime.

### Provider selection

Add `funasr` to the Settings selector as **Local FunASR (recommended)**. Make it the default for new configurations. Preserve existing values: existing `whisper` configurations continue to use Whisper until the user changes the setting.

When `funasr` is selected:

1. Start or reuse the local FunASR service.
2. Submit the recording to FunASR and retain its raw/corrected transcript artifacts.
3. If the service cannot start, times out, or returns an error, show the reason in task progress and run local Whisper with the existing model and prompt.

No execution path may silently select Volcengine when the configured provider is `funasr` or `whisper`.

### User experience

The Settings page explains that FunASR is the preferred offline Chinese engine and Whisper is automatic fallback. The active provider is saved immediately. Existing task progress distinguishes `FunASR`, `FunASR fallback`, and `Whisper`; users can understand which engine produced a transcript.

### Error handling

- Missing or invalid bundled files: fail the FunASR attempt with an actionable log message, then use Whisper.
- Service startup failure or timeout: stop/clean up only the FunASR child process, then use Whisper.
- FunASR recognition failure: preserve any auditable partial artifacts and use Whisper; do not overwrite existing successful transcript artifacts.
- Whisper failure after fallback: mark the ASR task failed with both causes and provide retry.

## Tests and acceptance

- Unit test provider defaults and accepted provider values.
- Unit test runtime resolution: bundled resource wins in packaged mode; developer overrides still work.
- Unit test a FunASR failure falls back to Whisper and never invokes Volcengine.
- Integration smoke test starts the bundled runtime, transcribes a short Chinese WAV, and verifies timestamped output.
- Packaging verification fails when the runtime executable or required model assets are absent.
- Manual test: select Local FunASR, transcribe a Chinese livestream recording, confirm task progress names the active engine; force a FunASR failure and confirm Whisper completes the same request.

## Non-goals

- No cloud ASR provider setup or Volcengine configuration changes.
- No replacement of existing transcript review, correction, or archive-playback behavior.
- No assumption that FunASR is always more accurate; the fallback is retained for operational resilience.
