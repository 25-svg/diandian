import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const source = readFileSync(
  new URL("../src-tauri/src/handlers/video.rs", import.meta.url),
  "utf8",
);

function functionBody(startMarker, endMarker) {
  const start = source.indexOf(startMarker);
  const end = source.indexOf(endMarker, start + startMarker.length);
  assert.notEqual(start, -1, `missing ${startMarker}`);
  assert.notEqual(end, -1, `missing ${endMarker}`);
  return source.slice(start, end);
}

const getSubtitle = functionBody(
  "pub async fn get_video_subtitle",
  "pub async fn generate_video_subtitle",
);
assert.match(
  getSubtitle,
  /resolve_video_transcript_context/,
  "video subtitle reads must resolve the canonical owner and alias identity",
);
assert.ok(
  getSubtitle.indexOf("canonical_artifacts_exist")
    < getSubtitle.indexOf('with_extension("srt")'),
  "video subtitle reads must prefer canonical transcript artifacts",
);
assert.match(
  getSubtitle,
  /load_from_dir[\s\S]*bundle\.corrected_srt/,
  "canonical reads must return corrected_srt",
);

const generation = functionBody(
  "async fn generate_video_subtitle_inner",
  "pub async fn update_video_subtitle",
);
assert.match(
  generation,
  /resolve_video_transcript_context/,
  "ASR generation must resolve the canonical owner and alias identity",
);
assert.match(
  generation,
  /initialize_from_asr_outputs[\s\S]*bundle\.corrected_srt/,
  "ASR generation must publish the safe canonical corrected transcript",
);
assert.doesNotMatch(
  generation,
  /update_video_subtitle_inner/,
  "ASR output must not pass through the manual subtitle writer",
);

const updateSubtitle = functionBody(
  "async fn update_video_subtitle_inner",
  "async fn write_legacy_video_subtitle",
);
assert.match(
  updateSubtitle,
  /resolve_video_transcript_context/,
  "manual subtitle updates must resolve the canonical owner and alias identity",
);
assert.match(
  updateSubtitle,
  /apply_manual_edit/,
  "manual subtitle updates must use the audited canonical store",
);
assert.match(
  updateSubtitle,
  /initialize_manual_import/,
  "first manual subtitles without legacy evidence must use explicit manual import provenance",
);
assert.match(
  updateSubtitle,
  /write_legacy_video_subtitle\(file, &bundle\.corrected_srt\)/,
  "legacy video.srt must mirror canonical corrected output",
);

const encodeSubtitle = functionBody(
  "async fn encode_video_subtitle_inner",
  "pub async fn generic_ffmpeg_command",
);
assert.match(
  encodeSubtitle,
  /resolve_video_transcript_context/,
  "subtitle burn-in must resolve the canonical owner and alias identity",
);
assert.match(
  encodeSubtitle,
  /load_from_dir[\s\S]*bundle\.corrected_srt[\s\S]*encode_video_subtitle/,
  "subtitle encoding must synchronize from canonical corrected output first",
);

console.log("Canonical video subtitle source check passed");
