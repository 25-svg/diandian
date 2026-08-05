# MiniMax 录播主播自动识别实施计划

> 执行方式：当前 Codex 连续开发；每阶段只运行聚焦测试，最终统一检查桌面端。

**目标：** 自动读取视频固定字幕牌中的主播姓名，保存到录播记录并允许人工校正。

**架构：** Rust 后端负责视频路径解析、抽帧、MiniMax-M3 多模态请求、结果校验和数据库写入；Svelte 列表负责顺序触发待识别任务、显示状态及人工编辑。人工结果优先于自动结果。

**技术栈：** Rust、Tauri、SQLite/sqlx、FFmpeg、reqwest、MiniMax Anthropic Messages API、Svelte/TypeScript。

---

## Task 1：主播识别领域规则

**文件：**
- 新建：`src-tauri/src/anchor_detection.rs`
- 修改：`src-tauri/src/main.rs`

1. 先写失败测试：
   - 两帧相同姓名得到 `confirmed/high`。
   - 单帧结果、多帧冲突和全空结果不得确认。
   - 模型输出带代码块时仍能解析 JSON。
   - 非法姓名或超长内容被拒绝。
2. 运行 `cargo test -p bili-shadowreplay anchor_detection`，确认因模块缺失或行为未实现而失败。
3. 实现最小 JSON 解析、姓名规范化和多帧一致性函数。
4. 重跑聚焦测试至通过。

## Task 2：数据库字段与人工锁定

**文件：**
- 修改：`src-tauri/src/database/video.rs`
- 修改：`src-tauri/src/main.rs`
- 修改所有 `VideoRow` 构造点。

1. 先写失败测试：
   - 新视频默认 `pending`。
   - 自动识别可写入姓名、来源、置信度和状态。
   - `manual` 来源不能被普通自动识别覆盖。
   - 人工保存会写入 `manual/confirmed`。
2. 新增 migration 27 和 `VideoRow` 字段。
3. 增加数据库方法：
   - `set_video_anchor_detection_running`
   - `save_video_anchor_detection`
   - `save_video_anchor_manual`
4. 更新所有视频构造点的默认字段。
5. 运行 `cargo test -p bili-shadowreplay database::video`。

## Task 3：MiniMax-M3 多帧识别命令

**文件：**
- 修改：`src-tauri/src/handlers/ai.rs`
- 修改：`src-tauri/src/handlers/video_editing.rs`
- 新建：`src-tauri/src/handlers/anchor_detection.rs`
- 修改：`src-tauri/src/handlers/mod.rs`
- 修改：`src-tauri/src/main.rs`

1. 先写失败测试：
   - 请求体使用 `MiniMax-M3`。
   - 请求包含 3 个 image content block 和严格识别提示。
   - 人工锁定时命令直接返回现有结果。
   - 路径或模型失败会写入 `failed`，但返回可读状态。
2. 提取可复用的“指定路径抽单帧为 base64”函数，使用唯一临时文件名。
3. 通过 canonical media resolver 读取本地或 NAS 视频。
4. 实现 `detect_video_anchor(video_id, force)` Tauri 命令。
5. 注册命令并运行聚焦 Rust 测试。

## Task 4：录播列表自动识别与人工编辑

**文件：**
- 修改：`src/lib/interface.ts`
- 新建：`src/lib/anchorDetection.ts`
- 新建：`src/lib/anchorDetection.test.ts`
- 修改：`src/page/Clip.svelte`

1. 先写失败测试：
   - 只有 `pending` 和允许重试的 `failed` 视频进入队列。
   - `manual` 记录永不进入自动队列。
   - 状态映射为小白可理解的中文。
2. 扩展 `VideoItem` 主播字段。
3. 录播列表加载后一次只识别一个待处理视频，失败不阻断后续项目。
4. 新增“主播”列、失败重试和人工修改弹窗。
5. 人工保存后即时更新列表并锁定来源。
6. 运行 `node --loader ts-node/esm src/lib/anchorDetection.test.ts` 和项目相关前端检查。

## Task 5：聚焦回归与桌面端验收

1. 运行主播识别、视频数据库和前端状态测试。
2. 运行受影响的现有视频测试，确认视频导入、录制入库、NAS 转存和列表查询未回归。
3. 启动桌面端，检查：
   - 待识别状态出现。
   - 后台识别时其他按钮仍可点击。
   - 成功后显示主播姓名和 MiniMax 来源。
   - 失败后可重试。
   - 人工修改后重启仍保留且不被覆盖。
4. 记录实际未能执行的外部条件，例如 MiniMax Key 不可用或样本视频没有字幕牌。

