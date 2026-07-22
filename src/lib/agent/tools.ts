import { tool } from "@langchain/core/tools";
import { z } from "zod";
import { invoke, invokeSensitive } from "../invoker";
import {
  default_profile,
  generateEventId,
  type ClipRangeParams,
  type Profile,
} from "../interface";

const platform_list = ["bilibili", "douyin"];

// @ts-ignore
const get_accounts = tool(
  async () => {
    const result = (await invoke("get_accounts")) as any;
    // hide cookies in result
    return {
      accounts: result.accounts.map((a: any) => {
        return {
          ...a,
          cookies: "********",
        };
      }),
    };
  },
  {
    name: "get_accounts",
    description: "Get all available accounts",
    schema: z.object({}),
  }
);

// @ts-ignore
const remove_account = tool(
  async ({ platform, uid }: { platform: string; uid: number }) => {
    const result = await invoke("remove_account", {
      platform,
      uid,
    });
    return result;
  },
  {
    name: "remove_account",
    description: "Remove an account",
    schema: z.object({
      platform: z
        .string()
        .describe(
          `The platform of the account. Can be ${platform_list.join(", ")}`
        ),
      uid: z.number().describe("The uid of the account"),
    }),
  }
);

// @ts-ignore
const add_recorder = tool(
  async ({
    platform,
    room_id,
    extra,
  }: {
    platform: string;
    room_id: string;
    extra: string;
  }) => {
    const result = await invoke("add_recorder", {
      platform,
      roomId: room_id,
      extra,
    });
    return result;
  },
  {
    name: "add_recorder",
    description: "Add a recorder",
    schema: z.object({
      platform: z
        .string()
        .describe(
          `The platform of the recorder. Can be ${platform_list.join(", ")}`
        ),
      room_id: z.string().describe("The room id of the recorder"),
      extra: z
        .string()
        .describe(
          "The extra of the recorder, should be empty for bilibili, and the sec_user_id for douyin"
        ),
    }),
  }
);

// @ts-ignore
const remove_recorder = tool(
  async ({ platform, room_id }: { platform: string; room_id: string }) => {
    const result = await invoke("remove_recorder", {
      platform,
      roomId: room_id,
    });
    return result;
  },
  {
    name: "remove_recorder",
    description: "Remove a recorder",
    schema: z.object({
      platform: z
        .string()
        .describe(
          `The platform of the recorder. Can be ${platform_list.join(", ")}`
        ),
      room_id: z.string().describe("The room id of the recorder"),
    }),
  }
);

// @ts-ignore
const get_recorder_list = tool(
  async () => {
    const result = await invoke("get_recorder_list");
    return result;
  },
  {
    name: "get_recorder_list",
    description: "Get the list of all available recorders",
    schema: z.object({}),
  }
);

// @ts-ignore
const get_recorder_info = tool(
  async ({ platform, room_id }: { platform: string; room_id: string }) => {
    const result = await invoke("get_room_info", { platform, roomId: room_id });
    return result;
  },
  {
    name: "get_recorder_info",
    description: "Get the info of a recorder",
    schema: z.object({
      platform: z.string().describe("The platform of the room"),
      room_id: z.string().describe("The room id of the room"),
    }),
  }
);

// @ts-ignore
const get_archives = tool(
  async ({
    room_id,
    offset,
    limit,
  }: {
    room_id: string;
    offset: number;
    limit: number;
  }) => {
    const archives = (await invoke("get_archives", {
      roomId: room_id,
      offset,
      limit,
    })) as any[];
    // hide cover in result
    return {
      archives: archives.map((a: any) => {
        return {
          ...a,
          cover: null,
          created_at: new Date(a.created_at).toLocaleString(),
        };
      }),
    };
  },
  {
    name: "get_archives",
    description: "Get the list of all archives of a recorder",
    schema: z.object({
      room_id: z.string().describe("The room id of the recorder"),
      offset: z.number().describe("The offset of the archives"),
      limit: z.number().describe("The limit of the archives"),
    }),
  }
);

// @ts-ignore
const get_archive = tool(
  async ({ room_id, live_id }: { room_id: string; live_id: string }) => {
    const result = (await invoke("get_archive", {
      roomId: room_id,
      liveId: live_id,
    })) as any;
    // hide cover in result, convert utc datetime to local datetime
    return {
      ...result,
      cover: null,
      created_at: new Date(result.created_at).toLocaleString(),
    };
  },
  {
    name: "get_archive",
    description: "Get the info of a archive",
    schema: z.object({
      room_id: z.string().describe("The room id of the recorder"),
      live_id: z.string().describe("The live id of the archive"),
    }),
  }
);

// @ts-ignore
const delete_archive = tool(
  async (args: {
    platform: string;
    room_id: string;
    live_id: string;
    idempotency_key?: string;
    confirmation_token?: string;
    trace_id?: string;
  }) => {
    const { room_id, live_id, ...securityArgs } = args;
    const result = await invokeSensitive("delete_archive", {
      ...securityArgs,
      roomId: room_id,
      liveId: live_id,
    });
    return result;
  },
  {
    name: "delete_archive",
    description: "Delete an archive",
    schema: z.object({
      platform: z
        .string()
        .describe(
          `The platform of the recorder. Can be ${platform_list.join(", ")}`
        ),
      room_id: z.string().describe("The room id of the recorder"),
      live_id: z.string().describe("The live id of the archive"),
    }),
  }
);

// @ts-ignore
const delete_archives = tool(
  async (args: {
    platform: string;
    room_id: string;
    live_ids: string[];
    idempotency_key?: string;
    confirmation_token?: string;
    trace_id?: string;
  }) => {
    const { room_id, live_ids, ...securityArgs } = args;
    const result = await invokeSensitive("delete_archives", {
      ...securityArgs,
      roomId: room_id,
      liveIds: live_ids,
    });
    return result;
  },
  {
    name: "delete_archives",
    description: "Delete multiple archives",
    schema: z.object({
      platform: z
        .string()
        .describe(
          `The platform of the recorder. Can be ${platform_list.join(", ")}`
        ),
      room_id: z.string().describe("The room id of the recorder"),
      live_ids: z.array(z.string()).describe("The live ids of the archives"),
    }),
  }
);

// @ts-ignore
const get_background_tasks = tool(
  async () => {
    const result = (await invoke("get_tasks")) as any[];
    return {
      tasks: result.map((t: any) => {
        return {
          ...t,
          created_at: new Date(t.created_at).toLocaleString(),
        };
      }),
    };
  },
  {
    name: "get_background_tasks",
    description: "Get the list of all background tasks",
    schema: z.object({}),
  }
);

// @ts-ignore
const delete_background_task = tool(
  async ({ id }: { id: string }) => {
    const result = await invoke("delete_task", { id });
    return result;
  },
  {
    name: "delete_background_task",
    description: "Delete a background task",
    schema: z.object({
      id: z.string().describe("The id of the task"),
    }),
  }
);

// @ts-ignore
const get_videos = tool(
  async ({ room_id }: { room_id: string }) => {
    const result = (await invoke("get_videos", { roomId: room_id })) as any[];
    return {
      videos: result.map((v: any) => {
        return {
          ...v,
          created_at: new Date(v.created_at).toLocaleString(),
        };
      }),
    };
  },
  {
    name: "get_videos",
    description: "Get the list of all videos of a room",
    schema: z.object({
      room_id: z.string().describe("The room id of the room"),
    }),
  }
);

// @ts-ignore
const get_all_videos = tool(
  async () => {
    const result = (await invoke("get_all_videos")) as any[];
    return {
      videos: result.map((v: any) => {
        return {
          ...v,
          created_at: new Date(v.created_at).toLocaleString(),
        };
      }),
    };
  },
  {
    name: "get_all_videos",
    description: "Get the list of all videos from all rooms",
    schema: z.object({}),
  }
);

// @ts-ignore
const get_video = tool(
  async ({ id }: { id: number }) => {
    const result = (await invoke("get_video", { id })) as any;
    return {
      video: {
        ...result,
        created_at: new Date(result.created_at).toLocaleString(),
      },
    };
  },
  {
    name: "get_video",
    description: "Get the info of a video",
    schema: z.object({
      id: z.number().describe("The id of the video"),
    }),
  }
);

// @ts-ignore
const get_video_cover = tool(
  async ({ id }: { id: number }) => {
    const result = await invoke("get_video_cover", { id });
    return {
      cover: result,
    };
  },
  {
    name: "get_video_cover",
    description: "Get the cover of a video in base64 format",
    schema: z.object({
      id: z.number().describe("The id of the video"),
    }),
  }
);

// @ts-ignore
const delete_video = tool(
  async (args: { id: number; idempotency_key?: string; confirmation_token?: string; trace_id?: string }) => {
    const result = await invokeSensitive("delete_video", args);
    return result;
  },
  {
    name: "delete_video",
    description: "Delete a video",
    schema: z.object({
      id: z.number().describe("The id of the video"),
    }),
  }
);

// @ts-ignore
const get_video_typelist = tool(
  async () => {
    const result = await invoke("get_video_typelist");
    return result;
  },
  {
    name: "get_video_typelist",
    description:
      "Get the list of all video types（视频分区） that can be selected on bilibili platform",
    schema: z.object({}),
  }
);

// @ts-ignore
const get_video_subtitle = tool(
  async ({ id }: { id: number }) => {
    const result = await invoke("get_video_subtitle", { id });
    return result;
  },
  {
    name: "get_video_subtitle",
    description:
      "Get the subtitle of a video, if empty, you can use generate_video_subtitle to generate the subtitle",
    schema: z.object({
      id: z.number().describe("The id of the video"),
    }),
  }
);

// @ts-ignore
const generate_video_subtitle = tool(
  async ({ id }: { id: number }) => {
    const result = await invoke("generate_video_subtitle", { id });
    return result;
  },
  {
    name: "generate_video_subtitle",
    description: "Generate the subtitle of a video",
    schema: z.object({
      id: z.number().describe("The id of the video"),
    }),
  }
);

// @ts-ignore
const encode_video_subtitle = tool(
  async ({ id, srt_style }: { id: number; srt_style: string }) => {
    const result = await invoke("encode_video_subtitle", {
      id,
      srtStyle: srt_style,
    });
    return result;
  },
  {
    name: "encode_video_subtitle",
    description: "Encode the subtitle of a video",
    schema: z.object({
      id: z.number().describe("The id of the video"),
      srt_style: z
        .string()
        .describe(
          "The style of the subtitle, it is used for ffmpeg -vf force_style, it must be a valid srt style"
        ),
    }),
  }
);

// @ts-ignore
const post_video_to_bilibili = tool(
  async ({
    uid,
    room_id,
    video_id,
    title,
    desc,
    tag,
    tid,
  }: {
    uid: number;
    room_id: string;
    video_id: number;
    title: string;
    desc: string;
    tag: string;
    tid: number;
  }) => {
    // invoke("upload_procedure", {
    //   uid: uid_selected,
    //   eventId: event_id,
    //   roomId: roomId,
    //   videoId: video.id,
    //   cover: video.cover,
    //   profile: profile,
    // })
    const event_id = generateEventId();
    let profile = default_profile();
    profile.title = title;
    profile.desc = desc;
    profile.tag = tag;
    profile.tid = tid;
    const result = await invoke("upload_procedure", {
      uid,
      eventId: event_id,
      roomId: room_id,
      videoId: video_id,
      profile,
    });
    return result;
  },
  {
    name: "post_video_to_bilibili",
    description: "Post a video to bilibili",
    schema: z.object({
      uid: z
        .number()
        .describe(
          "The uid of the user, it should be one of the uid in the bilibili accounts"
        ),
      room_id: z.string().describe("The room id of the room"),
      video_id: z.number().describe("The id of the video"),
      title: z.string().describe("The title of the video"),
      desc: z.string().describe("The description of the video"),
      tag: z
        .string()
        .describe(
          "The tag of the video, multiple tags should be separated by comma"
        ),
      tid: z
        .number()
        .describe(
          "The tid of the video, it is the id of the video type, you can use get_video_typelist to get the list of all video types"
        ),
    }),
  }
);

// @ts-ignore
const get_danmu_record = tool(
  async ({
    platform,
    room_id,
    live_id,
  }: {
    platform: string;
    room_id: string;
    live_id: string;
  }) => {
    const result = (await invoke("get_danmu_record", {
      platform,
      roomId: room_id,
      liveId: live_id,
    })) as any[];
    // remove ts from result
    return {
      danmu_record: result.map((r: any) => {
        return {
          ...r,
          ts: (r.ts / 1000).toFixed(1),
        };
      }),
    };
  },
  {
    name: "get_danmu_record",
    description:
      "Get the danmu record of a live, entry ts is relative to the live start time in seconds",
    schema: z.object({
      platform: z.string().describe("The platform of the room"),
      room_id: z.string().describe("The room id of the room"),
      live_id: z.string().describe("The live id of the live"),
    }),
  }
);

// @ts-ignore
const clip_range = tool(
  async ({
    reason,
    clip_range_params,
  }: {
    reason: string;
    clip_range_params: Omit<ClipRangeParams, 'ranges'> & {
      ranges: Array<{ start: number; end: number }>;
    };
  }) => {
    const event_id = generateEventId();
    const backendParams: ClipRangeParams = {
      ...clip_range_params,
    };

    const result = await invoke("clip_range", {
      eventId: event_id,
      params: backendParams,
    });
    return result;
  },
  {
    name: "clip_range",
    description:
      "Clip one or more ranges from a live stream to generate a video. When multiple ranges are provided, they are concatenated into a single video with an optional transition effect. You must provide a reason for your decision on params.",
    schema: z.object({
      reason: z
        .string()
        .describe(
          "The reason for the clip range, it will be shown to the user. You must offer a summary of the clip range content and why you choose this clip range."
        ),
      clip_range_params: z.object({
        room_id: z.string().describe("The room id of the room"),
        live_id: z.string().describe("The live id of the live"),
        ranges: z
          .array(
            z.object({
              start: z
                .number()
                .describe(
                  "The start time in SECONDS of the clip that relative to the live start time"
                ),
              end: z
                .number()
                .describe(
                  "The end time in SECONDS of the clip that relative to the live start time"
                ),
            })
          )
          .describe("Array of time ranges to clip. Supports single or multiple ranges."),
        danmu: z
          .boolean()
          .describe(
            "Whether to encode danmu, encode danmu will take a lot of time, so it is recommended to set it to false"
          ),
        local_offset: z
          .number()
          .describe(
            "The offset for danmu timestamp, it is used to correct the timestamp of danmu"
          ),
        title: z.string().describe("The title of the clip"),
        note: z.string().describe("The note of the clip"),
        cover: z.string().describe("Must be empty"),
        platform: z.string().describe("The platform of the clip"),
        fix_encoding: z
          .boolean()
          .describe(
            "Whether to fix the encoding of the clip, it will take a lot of time, so it is recommended to set it to false"
          ),
        transition: z
          .enum(["none", "fade", "dissolve", "wipeleft", "wiperight", "slideup", "slidedown"])
          .optional()
          .describe(
            "Transition effect between clips when multiple ranges are provided. Default: 'none' (direct cut)."
          ),
      }),
    }),
  }
);

// @ts-ignore
const get_recent_record = tool(
  async ({
    room_id,
    offset,
    limit,
  }: {
    room_id: string;
    offset: number;
    limit: number;
  }) => {
    const records = (await invoke("get_recent_record", {
      roomId: room_id,
      offset,
      limit,
    })) as any[];
    return {
      records: records.map((r: any) => {
        return {
          ...r,
          cover: null,
          created_at: new Date(r.created_at).toLocaleString(),
        };
      }),
    };
  },
  {
    name: "get_recent_record",
    description: "Get the list of recent records that bsr has recorded",
    schema: z.object({
      room_id: z.string().describe("The room id of the room"),
      offset: z.number().describe("The offset of the records"),
      limit: z.number().describe("The limit of the records"),
    }),
  }
);

// @ts-ignore
const get_recent_record_all = tool(
  async ({ offset, limit }: { offset: number; limit: number }) => {
    const records = (await invoke("get_recent_record", {
      roomId: 0,
      offset,
      limit,
    })) as any[];
    return {
      records: records.map((r: any) => {
        return {
          ...r,
          cover: null,
          created_at: new Date(r.created_at).toLocaleString(),
        };
      }),
    };
  },
  {
    name: "get_recent_record_all",
    description: "Get the list of recent records from all rooms",
    schema: z.object({
      offset: z.number().describe("The offset of the records"),
      limit: z.number().describe("The limit of the records"),
    }),
  }
);

// @ts-ignore
const generic_ffmpeg_command = tool(
  async ({ args }: { args: string[] }) => {
    const result = await invoke("generic_ffmpeg_command", { args });
    return result;
  },
  {
    name: "generic_ffmpeg_command",
    description: "Run a generic ffmpeg command",
    schema: z.object({
      args: z.array(z.string()).describe("The arguments of the ffmpeg command"),
    }),
  }
);

// @ts-ignore
const open_clip = tool(
  async ({ video_id }: { video_id: number }) => {
    const result = await invoke("open_clip", { videoId: video_id });
    return result;
  },
  {
    name: "open_clip",
    description: "Open a video preview window",
    schema: z.object({
      video_id: z.number().describe("The id of the video"),
    }),
  }
);

// @ts-ignore
const list_folder = tool(
  async ({ path }: { path: string }) => {
    const result = await invoke("list_folder", { path });
    return result;
  },
  {
    name: "list_folder",
    description: "List the files in a folder",
    schema: z.object({
      path: z.string().describe("The path of the folder"),
    }),
  }
);

// @ts-ignore
const get_archive_subtitle = tool(
  async ({
    platform,
    room_id,
    live_id,
  }: {
    platform: string;
    room_id: string;
    live_id: string;
  }) => {
    try {
      const result = await invoke("get_archive_subtitle", {
        platform,
        roomId: room_id,
        liveId: live_id,
      });
      return result;
    } catch (error) {
      // If subtitle doesn't exist, return a helpful message instead of throwing
      if (error.toString().includes("Subtitle not found")) {
        return {
          error: "subtitle_not_found",
          message: "该录播还没有生成字幕，可以使用 generate_archive_subtitle 工具生成字幕",
          platform,
          room_id,
          live_id,
        };
      }
      throw error;
    }
  },
  {
    name: "get_archive_subtitle",
    description:
      "Get the subtitle of a archive, it may not be generated yet, you can use generate_archive_subtitle to generate the subtitle",
    schema: z.object({
      platform: z.string().describe("The platform of the archive"),
      room_id: z.string().describe("The room id of the archive"),
      live_id: z.string().describe("The live id of the archive"),
    }),
  }
);

// @ts-ignore
const generate_archive_subtitle = tool(
  async ({
    platform,
    room_id,
    live_id,
  }: {
    platform: string;
    room_id: string;
    live_id: string;
  }) => {
    const result = await invoke("generate_archive_subtitle", {
      platform,
      roomId: room_id,
      liveId: live_id,
    });
    return result;
  },
  {
    name: "generate_archive_subtitle",
    description:
      "Generate the subtitle of a archive, it may take a long time, you should not call this tool unless user ask you to generate the subtitle. It can be used to overwrite the subtitle of a archive",
    schema: z.object({
      platform: z.string().describe("The platform of the archive"),
      room_id: z.string().describe("The room id of the archive"),
      live_id: z.string().describe("The live id of the archive"),
    }),
  }
);

// @ts-ignore
const extract_video_frames = tool(
  async ({
    video_id,
    timestamps,
    max_frames,
  }: {
    video_id: number;
    timestamps?: number[];
    max_frames?: number;
  }) => {
    const result = await invoke("extract_video_frames", {
      videoId: video_id,
      timestamps: timestamps || [],
      maxFrames: max_frames || 10,
    });
    return result;
  },
  {
    name: "extract_video_frames",
    description:
      "Extract frames from a video at specific timestamps or evenly distributed. Returns base64 encoded images for visual analysis. Use this to 'see' video content.",
    schema: z.object({
      video_id: z.number().describe("The id of the video"),
      timestamps: z
        .array(z.number())
        .optional()
        .describe(
          "Specific timestamps in seconds to extract frames. If not provided, frames will be evenly distributed"
        ),
      max_frames: z
        .number()
        .optional()
        .describe("Maximum number of frames to extract (default: 10)"),
    }),
  }
);

// @ts-ignore
const get_video_metadata = tool(
  async ({ video_id }: { video_id: number }) => {
    const result = await invoke("get_video_metadata", { videoId: video_id });
    return result;
  },
  {
    name: "get_video_metadata",
    description:
      "Get detailed metadata of a video including duration, resolution, codec, bitrate, fps, and file size",
    schema: z.object({
      video_id: z.number().describe("The id of the video"),
    }),
  }
);

// @ts-ignore
const analyze_danmu_highlights = tool(
  async ({
    platform,
    room_id,
    live_id,
    time_window,
    min_density,
  }: {
    platform: string;
    room_id: string;
    live_id: string;
    time_window: number;
    min_density: number;
  }) => {
    const result = await invoke("analyze_danmu_highlights", {
      platform,
      roomId: room_id,
      liveId: live_id,
      timeWindow: time_window,
      minDensity: min_density,
    });
    return result;
  },
  {
    name: "analyze_danmu_highlights",
    description:
      "Analyze danmu (comments) to find highlight moments based on comment density and keywords. Returns time ranges with high engagement that are good candidates for clipping.",
    schema: z.object({
      platform: z.string().describe("The platform of the live"),
      room_id: z.string().describe("The room id of the live"),
      live_id: z.string().describe("The live id of the live"),
      time_window: z
        .number()
        .describe(
          "Time window in seconds to analyze comment density (e.g., 30 for 30-second windows)"
        ),
      min_density: z
        .number()
        .describe(
          "Minimum comments per time window to consider as highlight (e.g., 10)"
        ),
    }),
  }
);

// @ts-ignore
const search_danmu_keywords = tool(
  async ({
    platform,
    room_id,
    live_id,
    keywords,
    context_seconds,
  }: {
    platform: string;
    room_id: string;
    live_id: string;
    keywords: string[];
    context_seconds: number;
  }) => {
    const result = await invoke("search_danmu_keywords", {
      platform,
      roomId: room_id,
      liveId: live_id,
      keywords,
      contextSeconds: context_seconds,
    });
    return result;
  },
  {
    name: "search_danmu_keywords",
    description:
      "Search for specific keywords in danmu and return timestamps with context. Useful for finding specific moments mentioned by viewers (e.g., '精彩', '666', '笑死').",
    schema: z.object({
      platform: z.string().describe("The platform of the live"),
      room_id: z.string().describe("The room id of the live"),
      live_id: z.string().describe("The live id of the live"),
      keywords: z
        .array(z.string())
        .describe("Keywords to search for in danmu content"),
      context_seconds: z
        .number()
        .describe(
          "Seconds of context to include before and after keyword match (e.g., 10)"
        ),
    }),
  }
);

// @ts-ignore
const merge_videos = tool(
  async ({
    video_ids,
    output_title,
    output_note,
    transition,
  }: {
    video_ids: number[];
    output_title: string;
    output_note: string;
    transition?: string;
  }) => {
    const result = await invoke("merge_videos", {
      videoIds: video_ids,
      outputTitle: output_title,
      outputNote: output_note,
      transition: transition || "none",
    });
    return result;
  },
  {
    name: "merge_videos",
    description:
      "Merge multiple videos into a single video. Videos will be concatenated in the order provided with optional transitions between clips. Useful for creating compilations or combining multiple clips.",
    schema: z.object({
      video_ids: z
        .array(z.number())
        .describe("Array of video IDs to merge, in order"),
      output_title: z.string().describe("Title for the merged video"),
      output_note: z.string().describe("Note for the merged video"),
      transition: z
        .enum(["none", "fade", "dissolve", "wipeleft", "wiperight", "slideup", "slidedown"])
        .optional()
        .describe(
          "Transition effect between clips. Options: 'none' (直接切换), 'fade' (淡入淡出), 'dissolve' (溶解), 'wipeleft' (左擦除), 'wiperight' (右擦除), 'slideup' (上滑), 'slidedown' (下滑). Default: 'none'"
        ),
    }),
  }
);

// @ts-ignore
const extract_video_audio = tool(
  async ({ video_id }: { video_id: number }) => {
    const result = await invoke("extract_video_audio", { videoId: video_id });
    return result;
  },
  {
    name: "extract_video_audio",
    description:
      "Extract audio from a video as a separate audio file. Useful for audio analysis or creating audio-only content.",
    schema: z.object({
      video_id: z.number().describe("The id of the video"),
    }),
  }
);

// @ts-ignore
const get_archive_metadata = tool(
  async ({
    platform,
    room_id,
    live_id,
  }: {
    platform: string;
    room_id: string;
    live_id: string;
  }) => {
    const result = await invoke("get_archive_metadata", {
      platform,
      roomId: room_id,
      liveId: live_id,
    });
    return result;
  },
  {
    name: "get_archive_metadata",
    description:
      "Get detailed metadata of an archive including duration, file size, video quality, and recording time",
    schema: z.object({
      platform: z.string().describe("The platform of the archive"),
      room_id: z.string().describe("The room id of the archive"),
      live_id: z.string().describe("The live id of the archive"),
    }),
  }
);

const get_review_samples = tool(
  async ({ category, baseline_only }: { category?: string; baseline_only?: boolean }) => {
    const rows = (await invoke("get_review_samples")) as any[];
    return rows.filter((row) => {
      if (category && row.category !== category) return false;
      if (baseline_only && row.is_b_baseline !== 1) return false;
      return true;
    });
  },
  {
    name: "get_review_samples",
    description:
      "Get saved live-commerce review samples and B baselines for calibration or A/B comparison. The result includes the full saved review content.",
    schema: z.object({
      category: z.string().optional().describe("Filter by exact product category"),
      baseline_only: z.boolean().optional().describe("Only return B baseline samples"),
    }),
  }
);

type ToolPermission = "read" | "write" | "dangerous" | "admin";

type ToolSecurityPolicy = {
  permission: ToolPermission;
  writes?: boolean;
  sensitive?: boolean;
  disabled?: boolean;
  resourceScoped?: boolean;
};

const toolSecurityPolicies: Record<string, ToolSecurityPolicy> = {
  get_accounts: { permission: "admin", resourceScoped: true },
  remove_account: { permission: "admin", writes: true, sensitive: true },
  add_recorder: { permission: "write", writes: true, resourceScoped: true },
  remove_recorder: { permission: "admin", writes: true, sensitive: true, resourceScoped: true },
  get_recorder_list: { permission: "read" },
  get_recorder_info: { permission: "read", resourceScoped: true },
  get_archives: { permission: "read", resourceScoped: true },
  get_archive: { permission: "read", resourceScoped: true },
  delete_archive: { permission: "dangerous", writes: true, sensitive: true, resourceScoped: true },
  delete_archives: { permission: "dangerous", writes: true, sensitive: true, resourceScoped: true },
  get_background_tasks: { permission: "read" },
  delete_background_task: { permission: "admin", writes: true, sensitive: true },
  get_videos: { permission: "read", resourceScoped: true },
  get_all_videos: { permission: "admin", resourceScoped: true },
  get_video: { permission: "read", resourceScoped: true },
  get_video_cover: { permission: "read", resourceScoped: true },
  delete_video: { permission: "dangerous", writes: true, sensitive: true, resourceScoped: true },
  get_video_typelist: { permission: "read" },
  get_video_subtitle: { permission: "read", resourceScoped: true },
  generate_video_subtitle: { permission: "write", writes: true, resourceScoped: true },
  encode_video_subtitle: { permission: "write", writes: true, resourceScoped: true },
  post_video_to_bilibili: { permission: "dangerous", writes: true, sensitive: true, resourceScoped: true },
  get_danmu_record: { permission: "read", resourceScoped: true },
  clip_range: { permission: "write", writes: true, resourceScoped: true },
  get_recent_record: { permission: "read", resourceScoped: true },
  get_recent_record_all: { permission: "admin", resourceScoped: true },
  generic_ffmpeg_command: { permission: "admin", writes: true, sensitive: true, disabled: true },
  open_clip: { permission: "read", resourceScoped: true },
  list_folder: { permission: "dangerous", sensitive: true, resourceScoped: true },
  get_archive_subtitle: { permission: "read", resourceScoped: true },
  generate_archive_subtitle: { permission: "write", writes: true, resourceScoped: true },
  extract_video_frames: { permission: "read", resourceScoped: true },
  get_video_metadata: { permission: "read", resourceScoped: true },
  analyze_danmu_highlights: { permission: "read", resourceScoped: true },
  search_danmu_keywords: { permission: "read", resourceScoped: true },
  merge_videos: { permission: "write", writes: true, resourceScoped: true },
  extract_video_audio: { permission: "write", writes: true, resourceScoped: true },
  get_archive_metadata: { permission: "read", resourceScoped: true },
  get_review_samples: { permission: "admin", resourceScoped: true },
};

const permissionRank: Record<ToolPermission, number> = {
  read: 1,
  write: 2,
  dangerous: 3,
  admin: 4,
};

function getCurrentAgentPermission(): ToolPermission {
  const configured = localStorage.getItem("mcp_agent_permission") as ToolPermission | null;
  if (configured && configured in permissionRank) return configured;
  return "read";
}

function generateTraceId() {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return `tr_${crypto.randomUUID()}`;
  }
  return `tr_${Date.now()}_${Math.random().toString(16).slice(2)}`;
}

function stableStringify(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(stableStringify).join(",")}]`;
  return `{${Object.keys(value as Record<string, unknown>)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${stableStringify((value as Record<string, unknown>)[key])}`)
    .join(",")}}`;
}

function hashInput(value: unknown): string {
  const input = stableStringify(value);
  let hash = 2166136261;
  for (let i = 0; i < input.length; i += 1) {
    hash ^= input.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

function redactSensitive(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(redactSensitive);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(
    Object.entries(value as Record<string, unknown>).map(([key, entry]) => {
      if (/cookie|csrf|token|secret|password|api[_-]?key|authorization/i.test(key)) {
        return [key, "********"];
      }
      return [key, redactSensitive(entry)];
    })
  );
}

function appendAuditLog(entry: Record<string, unknown>) {
  const safeEntry = redactSensitive(entry);
  const current = JSON.parse(localStorage.getItem("mcp_tool_audit_log") || "[]");
  current.push(safeEntry);
  localStorage.setItem("mcp_tool_audit_log", JSON.stringify(current.slice(-500)));
  invoke("console_log", {
    level: entry.status === "failed" || entry.status === "rejected" ? "warn" : "info",
    message: `[mcp-tool-audit] ${JSON.stringify(safeEntry)}`,
  }).catch(() => undefined);
}

function extendSchemaForPolicy(schema: any, policy: ToolSecurityPolicy) {
  if (!schema || typeof schema.extend !== "function") return schema;
  const extraFields: Record<string, any> = {
    trace_id: z.string().min(8).max(160).optional().describe("Optional caller-provided trace id"),
  };
  if (policy.writes) {
    extraFields.idempotency_key = z
      .string()
      .min(8)
      .max(128)
      .describe("Required idempotency key for write tools");
  }
  if (policy.sensitive) {
    extraFields.confirmation_token = z
      .string()
      .min(8)
      .max(160)
      .describe("Required second-confirmation token for sensitive tools");
  }
  return schema.extend(extraFields);
}

function validateConfirmation(toolName: string, confirmationToken?: string) {
  return confirmationToken === `confirm:${toolName}`;
}

async function invokeWithSecurity(originalTool: any, args: Record<string, any>, policy: ToolSecurityPolicy) {
  const startedAt = performance.now();
  const traceId = args.trace_id || generateTraceId();
  const inputHash = hashInput(args);
  const permission = getCurrentAgentPermission();
  const auditBase = {
    trace_id: traceId,
    tool_name: originalTool.name,
    permission_required: policy.permission,
    caller_permission: permission,
    input_hash: inputHash,
    idempotency_key: args.idempotency_key,
    confirmation_id: args.confirmation_token ? hashInput(args.confirmation_token) : undefined,
  };

  appendAuditLog({ ...auditBase, status: "received", created_at: new Date().toISOString() });

  try {
    if (policy.disabled) {
      throw new Error("PERMISSION_DENIED: tool is disabled by P0 security policy");
    }
    if (permissionRank[permission] < permissionRank[policy.permission]) {
      throw new Error(`PERMISSION_DENIED: ${originalTool.name} requires ${policy.permission}`);
    }
    if (policy.writes && !args.idempotency_key) {
      throw new Error("INVALID_ARGUMENT: idempotency_key is required for write tools");
    }
    if (policy.sensitive && !validateConfirmation(originalTool.name, args.confirmation_token)) {
      throw new Error(`CONFIRMATION_REQUIRED: confirmation_token must equal confirm:${originalTool.name}`);
    }

    const idempotencyStoreKey = policy.writes
      ? `mcp_idempotency:${originalTool.name}:${args.idempotency_key}`
      : "";
    if (policy.writes) {
      const existing = localStorage.getItem(idempotencyStoreKey);
      if (existing) {
        const parsed = JSON.parse(existing);
        if (parsed.input_hash !== inputHash) {
          throw new Error("CONFLICT: idempotency_key was already used with different arguments");
        }
        appendAuditLog({
          ...auditBase,
          status: "succeeded",
          idempotency_replay: true,
          duration_ms: Math.round(performance.now() - startedAt),
          created_at: new Date().toISOString(),
        });
        return parsed.result;
      }
    }

    const result = await originalTool.invoke(args);
    if (policy.writes) {
      localStorage.setItem(
        idempotencyStoreKey,
        JSON.stringify({
          input_hash: inputHash,
          result,
          created_at: new Date().toISOString(),
        })
      );
    }
    appendAuditLog({
      ...auditBase,
      status: "succeeded",
      duration_ms: Math.round(performance.now() - startedAt),
      created_at: new Date().toISOString(),
    });
    return result;
  } catch (error) {
    appendAuditLog({
      ...auditBase,
      status: "failed",
      duration_ms: Math.round(performance.now() - startedAt),
      error_message: error?.message || String(error),
      created_at: new Date().toISOString(),
    });
    throw error;
  }
}

function secureTool(originalTool: any) {
  const policy = toolSecurityPolicies[originalTool.name] || { permission: "read" as ToolPermission };
  return tool(
    async (args: Record<string, any>) => invokeWithSecurity(originalTool, args || {}, policy),
    {
      name: originalTool.name,
      description: `${originalTool.description}\n\nSecurity: permission=${policy.permission}; writes=${Boolean(policy.writes)}; sensitive=${Boolean(policy.sensitive)}; disabled=${Boolean(policy.disabled)}.`,
      schema: extendSchemaForPolicy(originalTool.schema, policy),
    }
  );
}

const rawTools = [
  get_accounts,
  remove_account,
  add_recorder,
  remove_recorder,
  get_recorder_list,
  get_recorder_info,
  get_archives,
  get_archive,
  delete_archive,
  delete_archives,
  get_background_tasks,
  delete_background_task,
  get_videos,
  get_all_videos,
  get_video,
  get_video_cover,
  delete_video,
  get_video_typelist,
  get_video_subtitle,
  generate_video_subtitle,
  encode_video_subtitle,
  post_video_to_bilibili,
  clip_range,
  get_danmu_record,
  get_recent_record,
  get_recent_record_all,
  generic_ffmpeg_command,
  open_clip,
  list_folder,
  get_archive_subtitle,
  generate_archive_subtitle,
  // New video editing tools
  extract_video_frames,
  get_video_metadata,
  analyze_danmu_highlights,
  search_danmu_keywords,
  merge_videos,
  extract_video_audio,
  get_archive_metadata,
  get_review_samples,
];

const tools = rawTools.map(secureTool);

export { tools };
