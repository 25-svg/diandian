export interface AnchorDetectionVideo {
  anchor_name: string;
  anchor_source: string;
  anchor_detection_status: string;
  anchor_detection_error?: string;
}

export function shouldAutoDetectAnchor(video: AnchorDetectionVideo): boolean {
  return (
    video.anchor_source !== "manual" &&
    ["pending", "not_requested"].includes(video.anchor_detection_status)
  );
}

export function canManuallyEditAnchor(video: AnchorDetectionVideo): boolean {
  return (
    video.anchor_detection_status === "failed" &&
    video.anchor_name.trim().length === 0
  );
}

export function anchorStatusLabel(video: AnchorDetectionVideo): string {
  if (
    video.anchor_detection_status === "confirmed" &&
    video.anchor_source === "manual"
  ) {
    return "人工确认";
  }
  if (
    video.anchor_detection_status === "confirmed" &&
    video.anchor_name.trim()
  ) {
    return "";
  }
  if (video.anchor_detection_status === "running") {
    return "正在识别";
  }
  if (video.anchor_detection_status === "failed") {
    return "未识别";
  }
  if (video.anchor_detection_status === "not_requested") {
    return "等待识别";
  }
  return "等待识别";
}
