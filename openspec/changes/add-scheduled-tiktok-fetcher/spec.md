# 规约：定时 TikTok 元数据采集（CronFetcher）

## 1. 数据模型

### 1.1 `ScheduledFetcher`（新增类型，TS 与 Rust 各一份镜像）

```ts
// src/types.ts
export interface ScheduledFetcher {
  id: string;                    // uuid
  name: string;                  // 自动生成：`<配置名> CronFetcher #N`
  target_url: string;            // 必填，必须含 'tiktok.com'
  window_days: number;           // ≥ 0
  window_hours: number;          // ≥ 0；window_days*24+window_hours ≥ 1
  interval_days: number;         // ≥ 0
  interval_hours: number;        // ≥ 0；interval_days*24+interval_hours ≥ 1
  output_dir: string;            // CSV 输出目录（绝对路径）
  max_fetch_num: number;         // 单次拉取的元数据条数上限，默认 1000，≥ 1
  num_meet_condition: number;    // 单次写入 CSV 的命中上限（满足时间窗口后取最新 N 条），默认 100，≥ 1
  tiktok_iid: string;            // yt-dlp `tiktok:app_info` 中的 IID；任务级配置，默认 "7501732030001269264"
  enabled: boolean;              // 默认 true；连续失败超 6 次时被系统置 false
  consecutive_failures: number;  // 连续失败次数；成功一次清零
  cooldown_until?: string | null;// 冷却结束时间 ISO8601；连续 3 次失败时设置为 now+1h，期间调度器跳过
  created_at: string;            // ISO8601
  updated_at: string;            // ISO8601
}
```

```rust
// src-tauri/src/scheduled_fetcher.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledFetcher {
    pub id: String,
    pub name: String,
    pub target_url: String,
    pub window_days: u32,
    pub window_hours: u32,
    pub interval_days: u32,
    pub interval_hours: u32,
    pub output_dir: String,
    #[serde(default = "default_max_fetch_num")]
    pub max_fetch_num: u32,
    #[serde(default = "default_num_meet_condition")]
    pub num_meet_condition: u32,
    #[serde(default = "default_tiktok_iid")]
    pub tiktok_iid: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub consecutive_failures: u32,
    #[serde(default)]
    pub cooldown_until: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
fn default_true() -> bool { true }
fn default_max_fetch_num() -> u32 { 1000 }
fn default_num_meet_condition() -> u32 { 100 }
fn default_tiktok_iid() -> String { "7501732030001269264".to_string() }
pub const DEFAULT_TIKTOK_IID: &str = "7501732030001269264";
```

### 1.2 `VideoConfig` 扩展

```rust
// src-tauri/src/config.rs
pub struct VideoConfig {
    // ... 既有字段 ...
    #[serde(default)]
    pub scheduled_fetchers: Vec<crate::scheduled_fetcher::ScheduledFetcher>,
}
```

```ts
// src/types.ts
export interface VideoConfig {
  // ... 既有字段 ...
  scheduled_fetchers?: ScheduledFetcher[];
}
```

> 必须使用 `#[serde(default)]` / TS `?:` 以保证旧 `app_data.json` 兼容。

### 1.3 `ScheduledRun`（执行记录）

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledRun {
    pub id: String,                    // uuid
    pub fetcher_id: String,
    pub fetcher_name: String,
    pub config_id: String,
    pub config_name: String,
    pub run_index: u32,                // App 内存计数器，从 1 开始
    pub trigger: RunTrigger,           // Manual | Scheduled
    pub status: RunStatus,             // Pending | FetchingList | Filtering | Writing | Success | Failed
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub fetched_total: u32,            // 从 yt-dlp 拿到的总数（单次执行内已去重）
    pub matched_in_window: u32,        // 在时间窗口内的数量（已截断到 num_meet_condition）
    pub new_appended: u32,             // 实际写入 CSV 的条数（= matched_in_window，每次新建文件不去重）
    pub csv_path: Option<String>,      // 本次执行产出的 CSV 绝对路径；失败时 None
    pub error_message: Option<String>,
    pub progress_message: String,      // 实时进展提示，便于 UI 展示
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RunTrigger { Manual, Scheduled }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus { Pending, FetchingList, Filtering, Writing, Success, Failed }
```

### 1.4 全局应用设置

> 本提案**无需**全局 AppSettings：原计划中的 `tiktok_iid` 已下沉到 `ScheduledFetcher.tiktok_iid` 任务级字段（每个任务独立配置，便于不同任务使用不同 IID）。如未来需要其他全局开关再行扩展。

## 2. 持久化布局

```
app_data.json
└── configs[].scheduled_fetchers[]   # tiktok_iid 已下沉到任务级

app_data_store/<config_id>/
├── tasks.json                       # 已有
├── used_tutorial_videos.json        # 已有
└── scheduled_runs.json              # 新增：该配置下所有定时任务的执行记录（30 天清理）

<用户配置的 output_dir>/
├── 1-20260531_103012.csv            # 新增：每次执行独立 CSV（轮次-时间）
├── 2-20260531_113015.csv
└── ...                              # 不去重、不追加、不覆盖
```

## 3. CSV 文件规范

- **路径**：`<fetcher.output_dir>/<run_index>-<YYYYMMDD_HHMMSS>.csv`
  - `<run_index>`：本进程内该 fetcher 的执行轮次（从 1 开始；App 重启清零）
  - `<YYYYMMDD_HHMMSS>`：任务执行开始时间（本地时区）
  - 示例：`output_dir/3-20260531_153012.csv`
- **每次执行新建独立文件**：互不追加、互不覆盖（`run_index` + 秒级时间戳天然防撞；同一秒内极端重启冲突时自动追加 `_<uuid前8位>` 后缀）
- **编码**：UTF-8 **with BOM**（首字节 `\xEF\xBB\xBF`）
- **分隔符**：逗号 `,`
- **换行**：`\r\n`（Windows/Excel 友好）
- **引号**：包含 `,` `"` `\r` `\n` 的字段使用 `"` 包裹，内部 `"` 转义为 `""`
- **写入流程**：BOM → 表头行 → 本次命中行（按 timestamp 倒序），写完即关闭文件
- **不做跨文件去重**：每个 CSV 是该次任务的独立快照

### 3.1 列定义（10 列，顺序固定）

| 序号 | 列名 | 类型 | 来源 / 规则 |
|---|---|---|---|
| 1 | `video_id` | string | yt-dlp `id` |
| 2 | `publish_time` | string | `timestamp` 转 `YYYY-MM-DD HH:MM:SS`（**本地时区**）|
| 3 | `like_count` | int | yt-dlp `like_count`，缺失为 `0` |
| 4 | `comment_count` | int | yt-dlp `comment_count`，缺失为 `0` |
| 5 | `repost_count` | int | yt-dlp `repost_count`，缺失为 `0` |
| 6 | `save_count` | int | yt-dlp `save_count`，缺失为 `0` |
| 7 | `view_count` | int | yt-dlp `view_count`，缺失为 `0` |
| 8 | `uploader` | string | yt-dlp `uploader` |
| 9 | `video_url` | string | `https://www.tiktok.com/@<uploader>/video/<id>` |
| 10 | `fetched_at` | string | 任务执行开始时间 `YYYY-MM-DD HH:MM:SS`（本地时区）|

## 4. yt-dlp 调用约定

### 4.1 sidecar 集成

- 二进制路径解析顺序：
  1. Tauri sidecar（生产环境）：`tauri::api::process::Command::new_sidecar("yt-dlp")`
  2. 系统 PATH（开发环境兜底）：`std::process::Command::new("yt-dlp")`
- `tauri.conf.json` 增加 `bundle.externalBin: ["binaries/yt-dlp"]`
- `capabilities/default.json` 增加 `shell:allow-execute` 权限
- GitHub Actions 打包前用 `curl` 下载对应平台 yt-dlp 到 `src-tauri/binaries/`
- `.gitignore` 增加 `src-tauri/binaries/yt-dlp*`，避免大二进制入仓

### 4.2 命令拼装

```
yt-dlp \
  --extractor-args "tiktok:app_info=<IID>/musical_ly/35.1.3/2023501030/1233" \
  --flat-playlist \
  --playlist-end <max_fetch_num> \
  --no-warnings \
  --print "%(id)s\t%(timestamp)s\t%(uploader)s\t%(like_count)s\t%(comment_count)s\t%(repost_count)s\t%(save_count)s\t%(view_count)s" \
  <target_url>
```

- `<max_fetch_num>` 取自 `fetcher.max_fetch_num`（默认 1000；用户可配，≥ 1，无强制上限但前端 > 2000 时弹警告）
- IID 取自 `fetcher.tiktok_iid`（任务级配置，默认 `7501732030001269264`；用户可在任务表单"高级"区块中覆盖）
- **不传** `--proxy` / `--cookies`（按 D22 决策）
- 输出格式用 `\t` 分隔，避免 TikTok 文案中的逗号干扰
- 解析每行 split('\t') → 严格 8 列，缺失字段为字符串 `"NA"`，转 int 时按 0 处理

### 4.3 退出码与错误识别

- 退出码非 0 → 视为失败
- stdout 0 行 → 视为"未拉到任何视频"，但不算失败（计为 success，run.fetched_total=0）
- stderr 含 `No working app info is available` → 给出明确错误提示："IID 已失效，请编辑该定时任务、在'高级 → TikTok IID'中替换"，前端弹 Toast + 跳转按钮

## 5. 调度框架

### 5.1 启动时

`main.rs` `setup` hook 中：
1. 加载 `AppData`，遍历 `configs[].scheduled_fetchers[]`
2. 对每个 `enabled == true` 的 fetcher，调用 `scheduler.register(fetcher, config_id, config_name)`
3. scheduler 内部为每个 fetcher 启动一个 `tokio::spawn` 循环：
   ```
   loop {
       wait until interval elapsed since last run
       acquire global mutex
       run_once(fetcher)
       sleep 5s
       release mutex
   }
   ```
4. 全局只有一把 `Arc<tokio::sync::Mutex<()>>`，所有 fetcher 共享，确保串行

### 5.2 注册/注销时机

- App 启动 → 加载所有 enabled fetcher
- 配置保存（save_configs）→ 重新构建调度图：
  - 新增 fetcher → register
  - 删除 fetcher → cancel
  - 修改 interval/window/url → cancel + register
  - enabled=false → cancel；enabled=true → register
- App 关闭 → drop scheduler，所有 spawn 自然终止

### 5.3 执行次数（run_index）

- 每个 fetcher 维护内存中的 `Arc<AtomicU32>` 计数器，初值 0
- 每次 `run_once` 开始时 `fetch_add(1)` 得到 1, 2, 3...
- App 重启后内存计数器从 0 重置；写入 ScheduledRun.run_index 时使用本进程内的计数

### 5.4 失败暂停（冷却 + 多级阈值）

- 调度循环每个 tick 开始时先检查：
  - 若 `cooldown_until.is_some() && now < cooldown_until` → 跳过本轮（仅写入一条带 progress_message="冷却中，剩余 X 分钟" 的占位 run，可选关闭）
  - 否则 `cooldown_until = None`，正常执行
- `run_once` 失败时 `fetcher.consecutive_failures += 1`，并按下表处置：

| consecutive_failures | 动作 |
|---|---|
| 1, 2 | 仅记录失败 run，下个 tick 正常重试 |
| 3 | 进入冷却态：`cooldown_until = now + 1h`；持久化；emit `scheduled-fetcher-cooldown { fetcher_id, cooldown_until }` |
| 4, 5 | 冷却结束后自动重试；继续失败仅累加计数（不重置 cooldown） |
| ≥ 6 | 彻底关停：`enabled = false`、清空 `cooldown_until`；持久化；emit `scheduled-fetcher-paused { fetcher_id, reason }`，前端高亮，需用户手动重新启用 |

- **任意一次成功**：`consecutive_failures = 0`、`cooldown_until = None`

## 6. 执行流程（核心算法）

```
fn run_once(fetcher, app_state):
    started_at = now_local()
    run_index = fetcher.run_counter.fetch_add(1) + 1   // 1, 2, 3...
    run = ScheduledRun { fetcher_id, run_index, status: Pending, started_at, ... }
    push run to app_state.scheduled_runs and emit event

    # ---- Stage 1: FetchingList ----
    update_status(FetchingList, "拉取列表中...")
    output, exit = exec_yt_dlp(fetcher.target_url, fetcher.tiktok_iid, fetcher.max_fetch_num)
    if exit != 0:
        mark_failed(parse_error_hint(stderr))
        return

    # ---- Stage 2: Filtering ----
    update_status(Filtering, "解析与排序中...")
    entries = parse_lines(output)            # Vec<Entry { id, ts, uploader, like, ... }>
    # 单次执行内按 aweme_id 去重（防 yt-dlp 重复返回）
    entries = dedup_by_id(entries)
    run.fetched_total = entries.len()
    entries.sort_by_key(|e| Reverse(e.ts))   # timestamp 倒序

    window_secs = (fetcher.window_days*24 + fetcher.window_hours) * 3600
    cutoff = started_at.timestamp() - window_secs
    matched = entries.iter().take_while(|e| e.ts >= cutoff).collect()
    # 命中上限截断：保留时间最新的 N 条
    if matched.len() > fetcher.num_meet_condition:
        matched = matched[..fetcher.num_meet_condition]
    run.matched_in_window = matched.len()

    # ---- Stage 3: Writing ----
    update_status(Writing, "写入 CSV 中...")
    csv_filename = format!("{}-{}.csv", run_index, started_at.format("%Y%m%d_%H%M%S"))
    csv_path = fetcher.output_dir.join(csv_filename)
    if csv_path.exists():                    // 极端同秒重启冲突
        csv_path = fetcher.output_dir.join(format!("{}-{}_{}.csv",
            run_index, started_at.format("%Y%m%d_%H%M%S"), short_uuid()))
    write_bom_and_header(csv_path)
    write_rows(csv_path, matched, fetched_at=started_at)   // 不去重、新文件全量写入

    run.new_appended = matched.len()
    run.csv_path = csv_path                  // 写入到 ScheduledRun 便于 UI 跳转
    mark_success()
```

## 7. Tauri Commands（前后端 IPC）

| 命令 | 入参 | 返回 | 说明 |
|---|---|---|---|
| `list_scheduled_runs` | `config_ids?: string[]` | `ScheduledRun[]` | 按配置筛选；不传则返回全部，按 started_at 倒序 |
| `trigger_scheduled_fetcher_test` | `fetcher_id: string` | `run_id: string` | 触发"测试"按钮，返回新 run id 用于前端订阅进度 |
| `pick_folder_for_fetcher` | — | `string \| null` | 复用现有的目录选择能力（若已存在则直接复用，不重复实现）|
| `open_csv_in_finder` | `csv_path: string` | `()` | 用系统资源管理器/Finder 打开 CSV 所在目录（便于用户查看产出）|

### 7.1 Tauri Events（后端推送）

| 事件名 | payload | 触发时机 |
|---|---|---|
| `scheduled-run-update` | `ScheduledRun` | 每次 run 状态变更时推送（FetchingList/Filtering/Writing/Success/Failed）|
| `scheduled-fetcher-cooldown` | `{ fetcher_id, cooldown_until }` | 连续 3 次失败进入 1h 冷却时 |
| `scheduled-fetcher-paused` | `{ fetcher_id, reason }` | 连续失败 ≥ 6 次彻底关停时（enabled=false）|
| `scheduled-fetcher-iid-invalid` | `{ fetcher_id }` | 检测到 stderr `No working app info`，提示用户更新该任务的 IID |

## 8. 前端 UI

### 8.1 ConfigModal 增加「定时任务」子卡片

- 位置：教材素材配置区块下方
- 卡片标题："定时任务（CronFetcher）"
- 内容：
  - 已配置任务列表（行内展示：name / target_url 缩略 / 频率 / 窗口 / 启用开关 / 编辑 / 删除 / 测试）
  - 底部"添加定时任务"按钮，点击展开内嵌表单
- 表单字段：见 §1.1 ScheduledFetcher，所有 number 输入框 `min=0`（`max_fetch_num` / `num_meet_condition` 为 `min=1`），提交前前端校验：
  - target_url 必须包含 `tiktok.com`
  - `window_days*24+window_hours >= 1` 否则提示"采集窗口至少 1 小时"
  - `interval_days*24+interval_hours >= 1` 否则提示"执行频率至少 1 小时"
  - `max_fetch_num >= 1`；当 > 2000 时弹出二次确认警示"过大值可能触发 TikTok 风控"
  - `num_meet_condition >= 1`
  - `output_dir` 非空且存在
  - `tiktok_iid` 非空（默认值即 `7501732030001269264`，用户编辑时不能清空）
- 表单内"高级（折叠）"区块包含：
  - **TikTok IID**：text 输入；默认显示当前值；旁置 ⓘ 提示"yt-dlp `tiktok:app_info` 中的 IID。当采集失败提示 IID 失效时，可在 yt-dlp 社区 issue 中获取新 IID 并填入此处。" + 「重置为默认」按钮（一键填回常量 `7501732030001269264`）
  - 后续如需扩展 IID 之外的高级参数也置于此区块
- 测试按钮 → 调用 `trigger_scheduled_fetcher_test` → 弹窗订阅 `scheduled-run-update`，按 status/progress_message 实时更新 UI；用户可手动关闭弹窗

### 8.2 主页 TaskList 改为左右两栏

- 容器使用 `grid-cols-2` 等比，最小宽度断点降级为单列堆叠
- 左栏：原 `<TaskList tasks={...} />`（保持原样）
- 右栏：新组件 `<ScheduledRunsList />`
  - 顶部下拉多选「按配置筛选」（默认全部）
  - 列表项展示：fetcher_name / config_name / started_at / 「第 N 次」/ status badge / progress_message / new_appended（成功后）+ "查看 CSV" 按钮（成功时显示，点击调用 `open_csv_in_finder`）/ error_message（失败时高亮红色）
  - 列表按 `started_at` 倒序

### 8.3 IID 失效告警

- 监听 `scheduled-fetcher-iid-invalid` 事件
- 弹出 Toast："任务 `<fetcher_name>` 的 TikTok IID 似乎已失效，请编辑该任务并在'高级 → TikTok IID'中替换为最新值"
- Toast 含「立即编辑」按钮，点击直接打开该 fetcher 的编辑表单并定位到高级区块
- **不再有全局 SettingsModal**（IID 已下沉到任务级）

## 9. 兼容性与迁移

- 老 `app_data.json` 加载：`scheduled_fetchers` 缺失 → 默认空数组；旧任务记录中缺 `max_fetch_num` / `num_meet_condition` / `tiktok_iid` / `cooldown_until` 字段 → 全部走 `#[serde(default)]` 函数填默认
- 不需要任何主动迁移脚本

## 10. 日志与可观测性

- 后端 `info!` 日志关键节点：
  - 注册/注销 fetcher
  - 每次 run 的 started/finished + 各阶段耗时
  - yt-dlp 命令输出的前 5 行（脱敏）和退出码
- 前端在 ScheduledRunsList 提供"查看日志"链接，点击展开 run.progress_message 全文 + error_message
- yt-dlp 完整 stdout/stderr 临时保存到 `app_data_store/<config_id>/scheduled_runs.json` 中对应 run 的 `error_message` 字段（仅失败时）；成功时不保留以节省空间
