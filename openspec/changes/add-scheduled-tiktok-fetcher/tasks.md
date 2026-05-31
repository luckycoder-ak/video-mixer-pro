# 任务清单：定时 TikTok 元数据采集（CronFetcher）

> 标记规则：[ ] 待办 / [x] 完成 / [~] 进行中 / [!] 阻塞

## P0 - 基础设施

- [ ] **T1** 在 `Cargo.toml` 添加依赖：`csv = "1.3"`（CSV 写入），确认 `tokio` 含 `sync` feature；如缺失则补上
- [ ] **T2** 在 `src-tauri/binaries/` 放置 yt-dlp 二进制（macOS 开发期手动放置）；目录入 `.gitignore`
- [ ] **T3** 修改 `src-tauri/tauri.conf.json`：`bundle.externalBin: ["binaries/yt-dlp"]`
- [ ] **T4** 修改 `src-tauri/capabilities/default.json`：增加 `shell:allow-execute` 权限并将 yt-dlp 加入允许列表
- [ ] **T5** 修改 `.github/workflows/build-windows.yml`：在 build 之前用 `Invoke-WebRequest` 下载 `yt-dlp.exe` 到 `src-tauri/binaries/yt-dlp-x86_64-pc-windows-msvc.exe`；macOS workflow 同理（如有）
- [ ] **T6** 编写启动期 yt-dlp 健康检查 `check_yt_dlp_health() -> bool`，加入 `AppState`（false 时 scheduler 不工作但不报错）

## P1 - 数据模型

- [ ] **T7** 新建 `src-tauri/src/scheduled_fetcher.rs`，定义：
  - [ ] `ScheduledFetcher` 结构体，字段含：
    - `id`, `name`, `target_url`, `window_days`, `window_hours`, `interval_days`, `interval_hours`, `output_dir`
    - `#[serde(default = "default_max_fetch_num")] max_fetch_num: u32`（默认 1000）
    - `#[serde(default = "default_num_meet_condition")] num_meet_condition: u32`（默认 100）
    - `#[serde(default = "default_tiktok_iid")] tiktok_iid: String`（默认 `7501732030001269264`）
    - `#[serde(default = "default_true")] enabled: bool`
    - `#[serde(default)] consecutive_failures: u32`
    - `#[serde(default)] cooldown_until: Option<DateTime<Utc>>`
    - `created_at`, `updated_at`
  - [ ] 默认函数 `default_true / default_max_fetch_num / default_num_meet_condition / default_tiktok_iid`
  - [ ] 常量 `pub const DEFAULT_TIKTOK_IID: &str = "7501732030001269264";`
  - [ ] `ScheduledRun` 结构体（含 `csv_path: Option<String>`，**无** `skipped_duplicates`）
  - [ ] `RunTrigger / RunStatus` 枚举（均 `#[serde(rename_all = "snake_case")]`）
  - [ ] **不**定义 `AppSettings` —— IID 已下沉到任务级
- [ ] **T8** 修改 `src-tauri/src/config.rs::VideoConfig` 增加 `#[serde(default)] pub scheduled_fetchers: Vec<ScheduledFetcher>`
- [ ] **T9** （已删除——`AppData` 不需要 `settings` 字段）
- [ ] **T10** 修改 `src/types.ts`：
  - 增加 `ScheduledFetcher`（含 `max_fetch_num` / `num_meet_condition` / `tiktok_iid` / `cooldown_until` 字段）/ `ScheduledRun`（含 `csv_path?: string`）/ `RunTrigger` / `RunStatus`
  - `VideoConfig.scheduled_fetchers?: ScheduledFetcher[]`
  - `createDefaultConfig` 默认空数组
- [ ] **T11** Rust 单测：`VideoConfig` 兼容反序列化旧 JSON（缺 `scheduled_fetchers`；scheduled_fetchers 中缺 `max_fetch_num` / `num_meet_condition` / `tiktok_iid` / `cooldown_until`）

## P2 - 核心采集逻辑

- [ ] **T12** 实现 `validate_target_url(url) -> Result<(), String>` —— 仅允许 `https://www.tiktok.com|tiktok.com|m.tiktok.com` 前缀
- [ ] **T13** 实现 `validate_iid_format(iid) -> Result<(), String>` —— regex `^\d{10,30}$`
- [ ] **T14** 实现 `exec_yt_dlp(target_url: &str, iid: &str, max_fetch_num: u32) -> Result<(String, String), YtDlpError>`
  - 优先 sidecar，失败回退系统 PATH
  - args: `--extractor-args "tiktok:app_info=<iid>/musical_ly/35.1.3/2023501030/1233" --flat-playlist --playlist-end <max_fetch_num> --no-warnings --print "<tab-separated 8 fields>" <url>`
  - 返回 (stdout, stderr)；非 0 退出码 → Err；stderr 匹配 `No working app info` 时 Err 中标记 `IidInvalid` 类型
- [ ] **T15** 实现 `parse_entries(stdout: &str) -> Vec<TikTokEntry>` —— 严格 8 列 `\t` split；缺失/NA 记 0
- [ ] **T16** 实现 `dedup_within_run(entries) -> Vec<TikTokEntry>` —— 按 `id` 字段去重，保留首次出现
- [ ] **T17** 实现 `filter_in_window_and_truncate(entries, window_secs, now_ts, num_meet_condition) -> Vec<TikTokEntry>`
  - 按 timestamp 倒序排序
  - take_while(ts >= now - window_secs)
  - 命中数 > `num_meet_condition` 时截断为前 N 条
- [ ] **T18** 实现 `build_csv_filename(run_index: u32, started_at: DateTime<Local>) -> String` —— 格式 `<run_index>-<YYYYMMDD_HHMMSS>.csv`；调用方在文件已存在时追加 `_<uuid前8位>` 后缀
- [ ] **T19** 实现 `write_csv_new_file(output_dir: &Path, csv_filename: &str, entries: &[TikTokEntry], fetched_at: DateTime<Local>) -> Result<PathBuf, ...>`
  - `fs::create_dir_all(output_dir)`
  - 新建文件，写 BOM + 表头 + 全部数据行
  - 使用 `csv::WriterBuilder` 配置 `terminator(Terminator::CRLF)` 与 `quote_style(QuoteStyle::Necessary)`
- [ ] **T20** 实现 `run_once(fetcher, run_index, app_state, app_handle) -> ScheduledRun` 整合上述
  - Stage 1: FetchingList → exec_yt_dlp（传 `fetcher.tiktok_iid` 与 `fetcher.max_fetch_num`）
  - Stage 2: Filtering → parse_entries → dedup_within_run → filter_in_window_and_truncate（传 `fetcher.num_meet_condition`）
  - Stage 3: Writing → build_csv_filename + 冲突检查 + write_csv_new_file → run.csv_path 填入
  - 失败时若 stderr 命中 IID 失效，emit `scheduled-fetcher-iid-invalid`
- [ ] **T21** 单测覆盖 T15/T16/T17/T18/T19 五个核心函数（含特殊字符、边界、空集、命中截断、同秒文件冲突）

## P3 - 调度器

- [ ] **T22** 实现 `Scheduler` 结构（`Arc<Mutex<HashMap<fetcher_id, JoinHandle>>>` + `Arc<tokio::sync::Mutex<()>>` + `HashMap<fetcher_id, AtomicU32>`）
- [ ] **T23** 实现 `Scheduler::register(fetcher, config_id, config_name, app_state, app_handle)` —— spawn 循环（含冷却态检查）
- [ ] **T24** 实现 `Scheduler::cancel(fetcher_id)`（清理 run_counter、consecutive_failures 内存状态）
- [ ] **T25** 实现 `Scheduler::run_now(fetcher_id, app_handle) -> run_id`（测试按钮路径，复用 run_once）
- [ ] **T26** 实现 `Scheduler::boot_from_app_data(app_data, app_state, app_handle)` —— 启动时全量注册
- [ ] **T27** 实现 `Scheduler::reconcile_after_save(new_configs)` —— save_configs 后 diff register/cancel
- [ ] **T28** 失败状态机：
  - failures += 1（成功一次清零）
  - == 3：`cooldown_until = now + 1h`，持久化，emit `scheduled-fetcher-cooldown`
  - >= 6：`enabled = false` 持久化，emit `scheduled-fetcher-paused`
  - 调度循环每 tick 先检查 `cooldown_until`，未到期跳过
- [ ] **T29** 修改 `main.rs::AppState` 加 `pub scheduler: Arc<Scheduler>`；`setup` hook 调用 `boot_from_app_data`
- [ ] **T30** 修改 `storage.rs::save_configs` 在持久化后调用 `scheduler.reconcile_after_save`

## P4 - 持久化扩展

- [ ] **T31** 实现 `load_scheduled_runs(config_id) -> Vec<ScheduledRun>` / `persist_scheduled_runs(config_id, runs)` （仿 tasks.json，30 天清理）
- [ ] **T32** 修改 `storage.rs::sync_config_store_in_dir`：删除配置时一并清理 `scheduled_runs.json`（无需清理 seen_ids，本提案已删除该机制）
- [ ] **T33** 单测：`scheduled_runs` 30 天清理；ScheduledFetcher 字段反序列化默认值兜底

## P5 - Tauri Commands & Events

- [ ] **T34** `#[tauri::command] list_scheduled_runs(state, config_ids: Option<Vec<String>>) -> Vec<ScheduledRun>`
- [ ] **T35** `#[tauri::command] trigger_scheduled_fetcher_test(state, fetcher_id) -> Result<String, String>` 返回 run_id
- [ ] **T36** `#[tauri::command] open_csv_in_finder(csv_path: String) -> Result<(), String>` —— 用 `opener` crate 或平台原生命令打开 CSV 所在目录
- [ ] **T37** （已删除——不再需要 `update_app_settings` / `get_app_settings`，IID 随 ScheduledFetcher 自动持久化）
- [ ] **T38** 在 `main.rs::invoke_handler` 注册新增命令（T34/T35/T36）
- [ ] **T39** Events：
  - `scheduled-run-update`：run_once 各阶段切换时 emit
  - `scheduled-fetcher-cooldown`：T28 第 3 次失败时 emit
  - `scheduled-fetcher-paused`：T28 第 6 次失败时 emit
  - `scheduled-fetcher-iid-invalid`：T20 检测到 IID 失效错误时 emit

## P6 - 前端 UI

- [ ] **T40** 新建 `src/components/ScheduledFetcherCard.tsx` —— 嵌入 ConfigModal
  - 列表展示已配置 fetchers + 操作按钮（编辑、删除、测试、启用开关）
  - 内嵌"添加"按钮 → 展开表单
- [ ] **T41** 新建 `src/components/ScheduledFetcherForm.tsx` —— 表单字段 + 校验
  - URL 校验：包含 `tiktok.com`
  - 窗口/频率：`days*24+hours >= 1`
  - **`max_fetch_num` 输入框 `min=1`；> 2000 时弹二次确认警示**
  - **`num_meet_condition` 输入框 `min=1`**
  - **"高级（折叠）"区块**：含 `tiktok_iid` text 输入（默认填 `7501732030001269264`）+ 「重置为默认」按钮 + ⓘ 提示文案
  - 输出目录：复用现有目录选择 IPC
  - **任务命名规则**：新增任务时名称自动填入 `<config.name> CronFetcher #N`，N 为该 config 下相同前缀的最大序号 +1（用户可手动修改）
- [ ] **T42** 新建 `src/components/ScheduledFetcherTestModal.tsx` —— 测试弹窗 + 订阅 `scheduled-run-update`，成功时显示"查看 CSV"按钮
- [ ] **T43** 修改 `src/components/ConfigModal.tsx` 在"教材素材配置"区块下方挂载 `<ScheduledFetcherCard>`
- [ ] **T44** 新建 `src/components/ScheduledRunsList.tsx`
  - 顶部多选下拉「按配置筛选」
  - 列表项：fetcher_name / config_name / started_at / 第 N 次 / status badge / progress_message / new_appended / "查看 CSV" 按钮（成功时显示，调 `open_csv_in_finder`）/ error_message
  - 订阅 `scheduled-run-update` 实时更新
- [ ] **T45** 修改 `src/App.tsx`：把当前 TaskList 改为 `grid grid-cols-2`，左 TaskList，右 ScheduledRunsList
- [ ] **T46** IID 失效全局监听：在 App.tsx 监听 `scheduled-fetcher-iid-invalid` 事件，弹 Toast + "立即编辑" 按钮（点击打开对应 fetcher 的编辑表单并展开"高级"区块）
- [ ] **T47** 冷却 / 关停状态展示：在 ScheduledFetcherCard 中根据 `cooldown_until` / `enabled` 字段显示 badge（"冷却中"剩余分钟数 / "已停用"红色）
- [ ] **T48** （已删除——不再需要全局 SettingsModal）

## P7 - 文档

- [ ] **T49** 在 `README.md` 增加"定时元数据采集（CronFetcher）"章节，说明：
  - 功能简介
  - 前置条件（系统级代理 / IID）
  - 使用步骤（含 max_fetch_num / num_meet_condition / 任务级 IID 配置）
  - CSV 命名规则与跨执行不去重的快照语义
  - 故障排查（IID 失效 → 编辑任务高级区块替换 / yt-dlp 未就绪 / 冷却态恢复）
- [ ] **T50** 在 `AGENTS.md` 的"最近修改记录"中追加 v1.0.6 条目

## P8 - 提案对照检查（强制最后一步）

- [ ] **T51** 实现完毕后**逐条核对** `proposal.md::§4 决策摘要` 的 D1-D23，记录每条的实现位置 (file path + symbol)；如有差异，在交付说明中显式列出
- [ ] **T52** 检查 `spec.md` 中所有"必须"语句是否落地（grep 搜索"必须" + 对照实现）
- [ ] **T53** 跑 `cargo test -p video-mixer-pro` 全绿 + `npm run build` 编译通过
- [ ] **T54** macOS 本地 E2E：
  - 测试一次真实 TikTok 链接完整流程
  - 验证 CSV 文件命名为 `<run_index>-<YYYYMMDD_HHMMSS>.csv`、内容正确
  - 验证连续 2 次执行产出 2 份独立 CSV，同 video_id 允许重复
  - 验证 IID 故意填错 → 第 3 次冷却 / 第 6 次关停的状态机
- [ ] **T55** 编写交付说明：列出新增文件 / 修改文件 / 提案差异（若有）/ 测试结果

## 进度追踪

- 总任务数：55（其中 T9/T37/T48 已在本次需求调整中标记为"删除"）
- 实际待执行任务数：52
- 完成：0 / 52

## 阻塞登记

| 时间 | 任务 | 阻塞原因 | 处理 |
|---|---|---|---|
| — | — | — | — |
