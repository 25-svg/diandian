import { invoke } from "../lib/invoker";

// #[derive(serde::Deserialize, serde::Serialize, Clone, Debug, Default)]
// pub struct RoomInfo {
//     pub platform: String,
//     pub room_id: String,
//     pub room_title: String,
//     pub room_cover: String,
//     /// Whether the room is live
//     pub status: bool,
// }

// #[derive(serde::Deserialize, serde::Serialize, Clone, Debug, Default)]
// pub struct UserInfo {
//     pub user_id: String,
//     pub user_name: String,
//     pub user_avatar: String,
// }

export interface RoomInfo {
  platform: string;
  room_id: string;
  room_title: string;
  room_cover: string;
  status: boolean;
}

export interface UserInfo {
  user_id: string;
  user_name: string;
  user_avatar: string;
}

export interface RecorderInfo {
  room_info: RoomInfo;
  user_info: UserInfo;
  platform_live_id: string;
  live_id: string;
  recording: boolean;
  enabled: boolean;
}

export interface RecorderList {
  count: number;
  recorders: RecorderInfo[];
}

export interface Subtitle {
  open: 0 | 1;
  lan: string;
}

export interface Video {
  title: string;
  filename: string;
  desc: string;
  cid: number;
}

export interface VideoItem {
  id: number;
  room_id: string;
  cover: string;
  file: string;
  length: number;
  size: number;
  status: number;
  bvid: string;
  title: string;
  note: string;
  desc: string;
  tags: string;
  area: number;
  created_at: string;
  platform?: string;
  anchor_name: string;
  anchor_source: string;
  anchor_confidence: string;
  anchor_detection_status: string;
  anchor_detection_error: string;
  anchor_detected_at: string;
}

/** True for clipped MP4s under clips/ — not a full live session. */
export function isClipVideo(video: Pick<VideoItem, "platform" | "file"> | null | undefined): boolean {
  if (!video) return false;
  if ((video.platform || "").toLowerCase() === "clip") return true;
  const file = (video.file || "").replace(/\\/g, "/");
  return file.startsWith("clips/") || file.includes("/clips/");
}

export interface ReviewSample {
  id: number;
  sample_no: string;
  product: string;
  category: string;
  deal_status: string;
  evidence_strength: string;
  transcript_path: string;
  data_screenshot_path: string;
  review_status: string;
  is_b_baseline: number;
  ops_score: number | null;
  host_score: number | null;
  control_score: number | null;
  main_issue: string;
  notes: string;
  clip_type: string;
  source_video_path: string;
  review_file_path: string;
  transcription_quality: string;
  agent_version: string;
  calibration_score: number | null;
  fact_accuracy_score: number | null;
  key_action_score: number | null;
  oral_usability_score: number | null;
  training_value_score: number | null;
  review_content: string;
  video_id: number | null;
  created_at: string;
  updated_at: string;
}

export type ReviewSampleInput = Omit<
  ReviewSample,
  "id" | "is_b_baseline" | "created_at" | "updated_at"
> & {
  id?: number;
  is_b_baseline: boolean;
};

export interface Profile {
  videos: Video[];
  cover: string;
  cover43: string | null;
  title: string;
  copyright: 1 | 2;
  tid: number;
  tag: string;
  desc_format_id: number;
  desc: string;
  recreate: number;
  dynamic: string;
  interactive: 0 | 1;
  act_reserve_create: 0 | 1;
  no_disturbance: 0 | 1;
  no_reprint: 0 | 1;
  subtitle: Subtitle;
  dolby: 0 | 1;
  lossless_music: 0 | 1;
  up_selection_reply: boolean;
  up_close_reply: boolean;
  up_close_danmu: boolean;
  web_os: 0 | 1;
}

export function default_profile(): Profile {
  return {
    videos: [],
    cover: "",
    cover43: null,
    title: "",
    copyright: 1,
    tid: 27,
    tag: "",
    desc_format_id: 9999,
    desc: "",
    recreate: -1,
    dynamic: "",
    interactive: 0,
    act_reserve_create: 0,
    no_disturbance: 0,
    no_reprint: 0,
    subtitle: {
      open: 0,
      lan: "",
    },
    dolby: 0,
    lossless_music: 0,
    up_selection_reply: false,
    up_close_danmu: false,
    up_close_reply: false,
    web_os: 0,
  };
}

export interface Config {
  cache: string;
  output: string;
  primary_uid: number;
  live_start_notify: boolean;
  live_end_notify: boolean;
  clip_notify: boolean;
  post_notify: boolean;
  auto_cleanup: boolean;
  auto_subtitle: boolean;
  subtitle_generator_type: string;
  whisper_model: string;
  whisper_prompt: string;
  openai_api_endpoint: string;
  openai_api_key: string;
  volcengine_api_key: string;
  volcengine_app_id: string;
  volcengine_access_token: string;
  volcengine_resource_id: string;
  volcengine_boosting_table_id: string;
  volcengine_correct_table_id: string;
  admin_mode: boolean;
  clip_name_format: string;
  auto_generate: AutoGenerateConfig;
  status_check_interval: number;
  whisper_language: string;
  webhook_url: string;
  danmu_ass_options: Danmu2AssOptions;
  powerlive_key: string;
  knowledge_vault_path: string;
  nas_video_storage: NasVideoStorageConfig;
}

export interface NasVideoStorageConfig {
  enabled: boolean;
  root_path: string;
  archive_recordings: boolean;
  archive_imports: boolean;
  delete_local_after_archive: boolean;
}

export interface Danmu2AssOptions {
  font_size: number;
  opacity: number;
}

export interface AutoGenerateConfig {
  enabled: boolean;
  encode_danmu: boolean;
}

export interface DiskInfo {
  disk: string;
  total: number;
  free: number;
}

export interface VideoType {
  id: number;
  parent: number;
  parent_name: string;
  name: string;
  description: string;
  desc: string;
  intro_original: string;
  intro_copy: string;
  notice: string;
  copy_right: number;
  show: boolean;
  rank: number;
  children: Children[];
  max_video_count: number;
  request_id: string;
}

export interface Children {
  id: number;
  parent: number;
  parent_name: string;
  name: string;
  description: string;
  desc: string;
  intro_original: string;
  intro_copy: string;
  notice: string;
  copy_right: number;
  show: boolean;
  rank: number;
  max_video_count: number;
  request_id: string;
}

export interface Marker {
  offset: number;
  realtime: number;
  content: string;
}

export interface ProgressUpdate {
  id: string;
  content: string;
}

export interface ProgressFinished {
  id: string;
  success: boolean;
  message: string;
}

export interface SubtitleStyle {
  fontName: string;
  fontSize: number;
  fontColor: string;
  outlineColor: string;
  outlineWidth: number;
  alignment: number;
  marginV: number;
  marginL: number;
  marginR: number;
}

export function parseSubtitleStyle(style: SubtitleStyle): string {
  // Convert hex color to ASS/SSA format (&HBBGGRR)
  function hexToAssColor(hex: string): string {
    if (!hex.startsWith("#")) return hex;
    const r = hex.slice(1, 3);
    const g = hex.slice(3, 5);
    const b = hex.slice(5, 7);
    return `&H${b}${g}${r}`;
  }

  return `FontName=${style.fontName},FontSize=${
    style.fontSize
  },PrimaryColour=${hexToAssColor(
    style.fontColor
  )},OutlineColour=${hexToAssColor(style.outlineColor)},Outline=${
    style.outlineWidth
  },Alignment=${style.alignment},MarginV=${style.marginV},MarginL=${
    style.marginL
  },MarginR=${style.marginR}`;
}

export interface Range {
  start: number;
  end: number;
  activated?: boolean;
}

export interface ClipRangeParams {
  title: string;
  note: string;
  cover: string;
  platform: string;
  room_id: string;
  live_id: string;
  ranges: Range[];
  danmu: boolean;
  local_offset: number;
  fix_encoding: boolean;
  transition?: string;
}

export function generateEventId() {
  return Math.random().toString(36).substring(2, 15);
}

export async function clipRange(eventId: string, params: ClipRangeParams) {
  return await invoke("clip_range", { eventId, params });
}

export interface DanmuEntry {
  ts: number;
  content: string;
  user_id?: string | null;
  user_name?: string | null;
}

export interface RecorderList {
  count: number;
  recorders: RecorderInfo[];
}
