# Douyin Recorder Request Fingerprint Repair

**Date:** 2026-07-31
**Status:** Approved for implementation planning
**Scope:** Restore local automatic recording for configured Douyin rooms after a desktop-app restart.

## Problem

Configured rooms are enabled (`auto_start = true`) and can be displayed as live, but the recorder does not enter `RecordStart`. Logs show `API error: Request params error` from the Douyin room-information request. The request declares fixed browser fields (`MacIntel`, Chrome 122) while generating a random User-Agent header, creating an inconsistent anti-bot request fingerprint.

The UI then retains a generic live badge even though a stream URL was not obtained or the recorder did not start, making the failure invisible.

## Design

### Stable request fingerprint

Use one coherent, fixed Windows Chrome browser profile for the Douyin web room API:

- User-Agent, `browser_platform`, `browser_name`, and `browser_version` describe the same browser.
- The exact unsigned query string is reused for `a_bogus` generation and the request URL.
- Cookie handling remains unchanged and no credentials are logged.

### Recovery

When the web endpoint cannot produce valid room data, retain the existing H5 fallback. If the fallback reports invalid request/session parameters, refresh the owner `sec_uid` from the public live-room page once and retry the H5 endpoint once. Do not retry indefinitely.

### Recording state visibility

Extend the recorder status exposed to the room card with a bounded last-error message. When a live room cannot acquire a stream, show a local failure state such as `Pull failed; retrying` rather than only `Live in progress`. Clear the message when `RecordStart` succeeds.

### Safety

- No changes to stored accounts, cookies, recordings, or archives.
- Existing HLS retry behavior remains unchanged after a stream starts.
- Failure remains recoverable through the existing polling cycle.

## Tests

1. Unit test the signed web query and headers use the same browser fingerprint.
2. Unit test H5 recovery refreshes `sec_uid` once and does not loop.
3. Unit test a room-information failure is exposed as a recorder status error and cleared by a record start.
4. Run the recorder crate tests and the targeted front-end type/check suite.
5. Manual verification: restart the desktop app with a configured live Douyin room; confirm the card shows `Recording` after stream acquisition, or a concrete retry error if the upstream endpoint rejects the request.

## Non-goals

- No cloud ASR or Volcengine changes.
- No automatic cookie refresh or account login automation.
- No changes to prior recordings or archive indexing.
