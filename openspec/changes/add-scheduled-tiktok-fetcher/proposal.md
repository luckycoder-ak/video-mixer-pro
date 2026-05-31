# 提案：定时 TikTok 元数据采集（CronFetcher）

## 1. 背景与动机

当前 VideoMixer Pro 仅支持基于本地素材的视频混剪输出，作者获取 TikTok 上目标视频指标（点赞、评论、转发、收藏、播放）的工作完全依赖人工逐条打开浏览器，效率极低；且无法对一个标的（音乐页 / 作者页 / 话题页）做时间窗口内的连续观察。

本提案引入「**定时元数据采集（CronFetcher）**」能力：
- 在每个 `VideoConfig` 下可配置多条定时任务，周期性扫描指定 TikTok 链接
- 拉取符合时间窗口（最近 X 天 Y 小时）的视频元数据
- 仅写入用户指定目录下的 `cron_fetcher.csv`，**不下载视频文件**；同一 `video_id` 在不同执行轮次允许重复写入（用于观察指标随时间变化）
- App 内提供「定时任务执行列表」UI，按配置筛选，按启动时间倒序展示运行情况

## 2. 目标

- 为 `VideoConfig` 增加 `scheduled_fetchers: ScheduledFetcher[]` 字段，跟随 `app_data.json` 持久化
- App 启动时根据配置自动注册定时任务，关闭即停（无后台守护、无错过补跑、执行次数从 1 重新计数）
- 通过 yt-dlp sidecar 拉取 TikTok 元数据；`--extractor-args "tiktok:app_info=<IID>/musical_ly/35.1.3/2023501030/1233"` workaround 解决官方 broken 提取器
- 严格按 `timestamp` 倒序排序后做时间窗口过滤；拉取上限 `max_fetch_num`（默认 1000，用户可配，≥1）；窗口内命中上限 `num_meet_condition`（默认 100，用户可配，≥1）
- CSV 列固定 10 列；**不做跨执行去重**——每次执行严格按本次任务的"最新 N 条"独立输出，允许同一 `video_id` 在多次执行中重复写入（用 `fetched_at` 列区分快照时点）；单次执行内仍按 `aweme_id` 去重防止 yt-dlp 重复返回
- 多任务**串行执行**，单次任务间隔 ≥ 5s sleep；连续 3 次失败暂停该任务并告警
- 主页 `TaskList` 改为左右两栏：左栏=视频合成任务（保持原样），右栏=定时任务执行列表

## 3. 非目标

- ❌ 不下载视频本体（仅采集元数据）
- ❌ 不做后台守护进程；App 关闭即停
- ❌ 不做错过补跑（错过即跳过到下一个周期）
- ❌ 不支持 TikTok 之外的站点（Twitter/X、YouTube、Bilibili 等不在范围）
- ❌ 不内置代理配置（依赖系统级 VPN/TUN 模式；用户自行配置）
- ❌ 不做跨执行去重（每次执行独立输出最新 N 条；CSV 允许同一 video_id 在不同 `fetched_at` 时点重复出现）
- ❌ 不在打包仓库内提交 yt-dlp 二进制（由 GH Actions 在打包前下载）

## 4. 关键决策摘要（已与研发对齐）

| # | 决策 | 终值 |
|---|---|---|
| D1 | 抓取方案 | yt-dlp sidecar，路径解析顺序 sidecar > 系统 PATH（与 ffmpeg 一致）|
| D2 | 支持链接 | TikTok 域内 `/music/...`、`/@username`、`/tag/...`（yt-dlp 原生支持）|
| D3 | 时间字段 | yt-dlp 返回的 `timestamp`（Unix 秒，TikTok 视频发布时间）|
| D4 | 调度生命周期 | App 内 `tokio::time::interval`；启动注册 / 关闭即停；执行次数=内存计数器，重启清零 |
| D5 | 并发策略 | 全局单一执行队列 + `tokio::Mutex`；任务间至少 sleep 5s |
| D6 | 错过补跑 | 否 |
| D7 | 频率/窗口最小粒度 | 各自总和 ≥ 1 小时（前端校验）|
| D8 | 拉取上限 `max_fetch_num` | 用户可配，默认 1000，≥1（仅元数据；1000 条实测 ~3 分钟）|
| D8' | 命中上限 `num_meet_condition` | 用户可配，默认 100，≥1（满足时间窗口的最多写入条数；按 timestamp 倒序后取前 N）|
| D9 | 排序与过滤 | flat-playlist 后按 `timestamp` 倒序排序；从最新一条向后扫描，遇到第一个超出时间窗口的视频终止；若窗内数量 > `num_meet_condition`，截断保留最新 N 条 |
| D10 | 去重策略 | **不跨执行去重**：每次执行独立按"最新 N 条（命中时间窗口）"输出；CSV 允许同一 `video_id` 在不同 `fetched_at` 重复出现；单次执行内仍按 `aweme_id` 去重防 yt-dlp 重复返回 |
| D11 | CSV 文件命名 | **每次执行新建独立文件**：`<output_dir>/<run_index>-<YYYYMMDD_HHMMSS>.csv`；UTF-8 with BOM；CRLF 行尾；同秒冲突追加 `_<uuid前8位>` 后缀；不追加、不覆盖、不去重 |
| D12 | CSV 列 | 10 列：`video_id,publish_time,like_count,comment_count,repost_count,save_count,view_count,uploader,video_url,fetched_at` |
| D13 | video_url 拼接 | `https://www.tiktok.com/@<视频自身的uploader>/video/<id>` |
| D14 | 互动指标字段 | `like_count`/`comment_count`/`repost_count`/`save_count`/`view_count`（已实测可用）|
| D15 | 测试按钮 | 立即触发一次完整流程；弹窗实时进展；用户手动关闭 |
| D16 | 任务命名 | 自动：`<配置名> CronFetcher #N`（N 为该配置下序号）|
| D17 | 配置存储 | `VideoConfig.scheduled_fetchers: ScheduledFetcher[]` ↔ `app_data.json` |
| D18 | 执行记录存储 | `app_data_store/<config_id>/scheduled_runs.json`（保留 30 天，与 tasks.json 同清理策略）|
| D19 | 主页布局 | 左右两栏（左=视频合成任务列表，右=定时任务执行列表）|
| D20 | 失败暂停 | 连续 3 次失败时进入冷却态（`cooldown_until = now + 1h`）；冷却期内调度器跳过该任务；冷却期过后下一个调度 tick 自动重试，重试成功则失败计数清零；重试再失败则继续累计，连续失败超 6 次时彻底 `enabled=false` 并发 Event 通知用户 |
| D21 | IID 配置 | **任务级**：每个 `ScheduledFetcher` 都带 `tiktok_iid` 字段，默认 `7501732030001269264`，用户可在任务表单"高级"区块中覆盖；当采集失败原因匹配 `No working app info`/`Unable to extract` 关键词时，前端弹 Toast 提示用户更新该任务的 IID 并提供"立即编辑"跳转按钮；**不再设全局 SettingsModal** |
| D22 | 代理 | 不在 App 内配置；依赖系统 VPN/TUN |
| D23 | 任务命名规则 | 格式 `<配置名> CronFetcher #N`；N = 当前 `VideoConfig.scheduled_fetchers` 下相同 `<配置名> CronFetcher` 前缀的最大已用序号 +1（首个为 #1）；删除任务后序号不回收（即下一个仍取 max+1）；用户手动重命名后保留用户值，不再自动套规则 |

## 5. 影响与风险

### 5.1 兼容性
- `VideoConfig` 增加 `scheduled_fetchers` 字段必须使用 `#[serde(default)]`，旧 `app_data.json` 反序列化得到空数组，**完全向后兼容**
- 老用户首次升级无需任何迁移步骤

### 5.2 风险点
| 风险 | 概率 | 影响 | 缓解 |
|---|---|---|---|
| yt-dlp 提取器进一步 break | 中 | 高 | IID 用户可覆盖；连续 3 次失败暂停告警 |
| 内置 IID 失效 | 中 | 高 | 全局设置中允许手动替换 IID |
| TikTok 风控（高频请求被封 IP）| 低 | 中 | 频率下限 1h、串行 + 5s 间隔、不下载视频本体（请求量极低）|
| 用户没有系统级代理 | 中 | 高 | 文档明确说明前置条件；首次执行失败时给出代理排查提示 |
| CSV 文件被用户手动删除 | 低 | 低 | CSV 缺失时重建表头并继续追加；本次任务正常采集，无任何状态依赖 |
| 1000 条样本耗时 ~3 分钟 | — | 低 | 已与用户确认可接受；执行期间右栏 UI 实时显示进度；用户可调小 `max_fetch_num` 缩短耗时 |
| 用户把 `max_fetch_num` 调到很大（如 5000+）触发风控 | 中 | 中 | 前端弹出二次确认 + 文案警示；不强制硬上限以保留灵活性 |
| 二进制体积增加 ~12-15MB | 中 | 中 | GH Actions 打包前自动下载，不入仓 |

### 5.3 回滚策略
- 完全可回滚：删除 `scheduled_fetchers` 相关代码 + 字段；旧 `app_data.json` 反序列化兼容
- yt-dlp sidecar 注册失败时降级为「该功能整体禁用」，不影响视频合成主流程

## 6. 验收标准（高层）

- [ ] 配置编辑弹窗可新增/编辑/删除定时任务，前端表单全部校验生效
- [ ] App 启动后定时任务按频率自动触发；关闭 App 任务停止
- [ ] 测试按钮触发一次完整流程，弹窗实时显示进展，结束后用户可关闭
- [ ] CSV 表头 10 列正确，UTF-8 with BOM，Excel 中文不乱码
- [ ] 同一 video_id 在不同执行轮次允许重复出现于 CSV（`fetched_at` 列时点不同）；单次执行内不重复
- [ ] 主页右栏「定时任务执行列表」正常筛选 + 倒序展示
- [ ] 旧 `app_data.json` 加载兼容（无 `scheduled_fetchers` 字段）
- [ ] yt-dlp sidecar 在 macOS / Windows 双端均可启动
- [ ] 全部已确认决策（D1-D23）在实现中**逐项落地**，差异点必须在交付说明中显式列出
