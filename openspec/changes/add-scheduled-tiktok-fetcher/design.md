# 设计：定时 TikTok 元数据采集（CronFetcher）

## 1. 总体架构

```
┌──────────────────────────────────────────────────────────────────────────┐
│                              前端 (React/TS)                              │
│                                                                          │
│  ConfigModal                       App.tsx                               │
│  └─ ScheduledFetcherCard           └─ TaskList (grid-cols-2)             │
│     └─ FetcherForm                    ├─ <视频合成任务列表>（既有）       │
│        └─ "高级（折叠）" → IID         └─ <ScheduledRunsList>（新）       │
│                                                                          │
│  IID 失效 Toast (`scheduled-fetcher-iid-invalid` listener)               │
└──────────────────────────────────────────────────────────────────────────┘
                          │ Tauri IPC + Events
                          ▼
┌──────────────────────────────────────────────────────────────────────────┐
│                              后端 (Rust)                                  │
│                                                                          │
│  scheduled_fetcher.rs                                                    │
│  ├─ ScheduledFetcher / ScheduledRun / RunTrigger / RunStatus             │
│  ├─ Scheduler { handles: HashMap<fetcher_id, JoinHandle>,                │
│  │              global_mutex: Arc<Mutex<()>>,                            │
│  │              run_counters: HashMap<fetcher_id, AtomicU32> }           │
│  ├─ run_once(fetcher, app_state, app_handle)                             │
│  ├─ exec_yt_dlp(target_url, iid, max_fetch_num) -> Result<String, …>     │
│  ├─ parse_entries(stdout) -> Vec<TikTokEntry>                            │
│  ├─ dedup_within_run(entries) -> Vec<TikTokEntry>                        │
│  ├─ filter_in_window_and_truncate(entries, window_secs, num_meet)        │
│  ├─ write_csv_new_file(output_dir, run_index, started_at, entries)       │
│  └─ load/persist_scheduled_runs                                          │
│                                                                          │
│  storage.rs（扩展）                                                      │
│  └─ load_scheduled_runs / persist_scheduled_runs（仿 tasks.json 模式）   │
│                                                                          │
│  config.rs（扩展）                                                       │
│  └─ VideoConfig.scheduled_fetchers                                       │
│                                                                          │
│  main.rs                                                                 │
│  ├─ AppState 增加 scheduler: Arc<Scheduler>                              │
│  └─ setup hook: scheduler.boot_from_app_data()                           │
└──────────────────────────────────────────────────────────────────────────┘
                          │ Command::new_sidecar("yt-dlp")
                          ▼
┌──────────────────────────────────────────────────────────────────────────┐
│  yt-dlp sidecar binary（externalBin）                                     │
│  └─ TikTok 列表抓取（--flat-playlist + --extractor-args app_info）       │
└──────────────────────────────────────────────────────────────────────────┘
```

## 2. 关键设计决策（含权衡）

### 2.1 为什么用 yt-dlp sidecar 而不是自己实现 TikTok API

| 方案 | 优 | 劣 | 选择 |
|---|---|---|---|
| 自实现 TikTok API | 体积小 | 签名算法每月变；维护爆炸；TikTok 反爬重 | ❌ |
| 嵌无头浏览器（Playwright） | 适配性好 | 体积 ~150MB；启动慢；与 Tauri 哲学冲突 | ❌ |
| **yt-dlp sidecar** | 社区维护活跃；二进制 ~12MB；输出稳定 | 提取器偶尔 broken；需用户提供 IID workaround | ✅ |

### 2.2 为什么 IID 下沉到任务级而不是全局

实测 yt-dlp 默认无内置 IID，提取器报 `No working app info is available`。社区 workaround 是从真实 TikTok app 抓取一个 IID 并通过 `--extractor-args` 注入。但 IID 会随 TikTok 后端策略**逐渐失效**（实测有效期数月到数年不等）。

**为什么任务级而不是全局**：
- 用户可能同时运行多个 fetcher 任务，不同 IID 对应不同账号上下文，并发场景下隔离更安全
- 当某个 IID 失效时，可仅替换出问题任务的 IID，不影响其它任务
- 模型简化：避免引入额外的 `AppSettings` 结构和读写命令

设计：
- `ScheduledFetcher.tiktok_iid` 字段，默认值 `7501732030001269264`（开发期实测可用）
- 任务表单"高级（折叠）"区块允许覆盖，含「重置为默认」按钮
- 失效时（stderr 含 `No working app info`）emit `scheduled-fetcher-iid-invalid { fetcher_id }`，前端 Toast + "立即编辑" 跳转到该任务

### 2.3 为什么 max_fetch_num / num_meet_condition 用户可配（默认 1000 / 100）

- **max_fetch_num（默认 1000）**：实测 1000 条耗时 ~174s，是单一音乐页/作者页的常见覆盖上限；但部分用户场景可能样本更稀疏（小创作者、冷门 hashtag）只需 200 条即可，或者高频热门账号需要更大样本（5000+）观察长时段
- **num_meet_condition（默认 100）**：用户在窗口内通常只关心 Top 最新若干条；超出可能造成 CSV 行数过多、Excel 打开变慢
- 两者下限均为 1（输入框 `min=1`、Rust 反序列化默认函数 + 表单校验）
- `max_fetch_num > 2000` 时前端弹二次确认（风险提示，不强制硬上限）
- 耗时与 `max_fetch_num` 近似线性相关：经验公式 ~T(s) ≈ 0.17 × max_fetch_num

### 2.4 为什么串行而非并行

- TikTok 接口限频严格；并发并未提升单任务吞吐（每个任务都是大量分页）
- 串行 + 5s 间隔最大化降低风控风险
- 实现简单：一把全局 `tokio::Mutex` 即可
- 用户最多配置几个定时任务，串行不会引起明显延迟

### 2.5 为什么"测试按钮"也走完整流程而非 Dry Run

- 用户已确认（决策 D15）
- 真实流程才能验证 CSV 写入、网络是否通；Dry Run 容易让用户误判"成功"
- 风险低：测试按钮只是新建一份 CSV 文件（命名 `<run_index>-<timestamp>.csv`），不影响其它执行产物

### 2.6 CSV 加 BOM 与 \r\n 的取舍

- BOM：让 Excel 在 Windows 中文环境识别 UTF-8，否则中文乱码
- `\r\n`：Windows 用户主流；macOS Excel 也兼容
- 代价：在 macOS Numbers 中显示无差异；Linux awk/sed 处理时需注意（可接受，目标用户为 Excel）

### 2.7 为什么"每次执行新建独立 CSV"而非追加到单一 CSV

- 用户期望按"轮次"管理产物：CSV 名 `<run_index>-<时间>.csv` 一目了然
- 跨执行不去重 → 单一 CSV 容易膨胀，列固定为 10 列时无法在文件名上携带"快照时点"信息
- 独立文件天然支持：用户拿走/删除某次产物不影响其它；下载到云盘做版本管理也直观
- 简化代码：不再需要扫描已有 CSV 的 video_id 集合、不再需要 `seen_ids/<fetcher_id>.json` 文件、不再需要 BOM 仅首次写入的判断分支
- 代价：用户若想跨次合并需自行用 Excel/Python 拼接（可接受，可在文档中给出脚本提示）

### 2.8 为什么不做跨执行去重

- **快照语义**：每次执行都是该时点的"最新 N 条"独立观察。指标会随时间变化（点赞数会涨），同一 video_id 在不同 `fetched_at` 出现是有价值的趋势信号
- **下游分析友好**：Excel/Pandas 透视时按 (video_id, fetched_at) 维度天然透视；如需"最新一行"用户自己 max(fetched_at) 即可
- **简化容错**：CSV 删除/移动/损坏都不影响下次执行
- **单次执行内仍按 aweme_id 去重**：防 yt-dlp 在分页时偶发重复返回（实测低概率）

### 2.9 publish_time 用本地时区

- 用户主要看本地时间（"昨天发的"比"UTC 1779287260"直观）
- 用 `chrono::Local::from_timestamp(ts, 0)` 转换
- CSV 里保存为字符串无 TZ 后缀（用户用 Excel 比较时不会受 TZ 干扰）

### 2.10 失败次数为什么是"3 触发冷却 / 6 彻底关停"两阶梯

- 1 次太敏感（单次网络抖动即停）
- 5 次太迟（IID 真失效情况下白等几个小时）
- **3 次进入 1h 冷却**：网络抖动通常不会连续 3 次（不同时点）；IID 失效则会连续 3 次报同一错误。冷却期内调度器跳过该任务，避免 1h 内频繁请求触发 TikTok 风控
- **冷却结束自动重试**：解决"用户不在 App 边、IID 是临时性失效"场景，避免必须人工干预
- **6 次彻底关停**：冷却 + 自动重试后仍连续失败 → 几乎可断定 IID 永久失效或 yt-dlp 提取器被破坏；持久化 `enabled=false`，emit Event 引导用户人工介入

### 2.11 为什么取消"全局指标更新"概念

- 已采纳"每次执行新建 CSV + 不跨执行去重"模型，天然支持指标变化观察（同一 video_id 在多份 CSV 中以不同 fetched_at 出现）
- 用户透视分析时按 (video_id, fetched_at) 排序即可看到趋势，无需 INSERT/UPDATE 复杂度

## 3. 数据流详解

### 3.1 App 启动流程

```
main.rs::setup
  ├─ load_runtime_store() → AppData (含 configs[].scheduled_fetchers)
  ├─ AppState::new() with scheduler: Arc::new(Scheduler::new())
  ├─ scheduler.boot_from(app_data)
  │   for config in configs:
  │       for fetcher in config.scheduled_fetchers:
  │           if fetcher.enabled:
  │               scheduler.register(config, fetcher)
  └─ done
```

### 3.2 定时任务循环（单个 spawn）

```
loop {
    wait_until_next_tick(fetcher.interval)

    // 冷却态检查
    if let Some(until) = fetcher.cooldown_until {
        if now() < until {
            continue;  // 跳过本轮，下个 tick 再检
        }
        // 冷却结束，清空标记
        clear_cooldown_and_persist(fetcher_id);
    }

    let _guard = global_mutex.lock().await;
    let run_index = counter.fetch_add(1, Ordering::SeqCst) + 1;
    let run = run_once(fetcher.clone(), run_index, app_state.clone(), app_handle.clone()).await;
    persist_run_to_disk(config_id, &run);

    // 失败处置：3 触发冷却 / 6 彻底关停
    if run.status == Failed {
        let n = increment_failure_count(fetcher_id);
        match n {
            3 => {
                set_cooldown_until(fetcher_id, now() + Duration::hours(1));
                emit("scheduled-fetcher-cooldown", { fetcher_id, cooldown_until });
            }
            n if n >= 6 => {
                disable_fetcher_and_persist(fetcher_id);
                emit("scheduled-fetcher-paused", { fetcher_id, reason });
                break;
            }
            _ => {}
        }
        // 检测 IID 失效信号（独立于失败次数）
        if run.error_message.contains("No working app info") {
            emit("scheduled-fetcher-iid-invalid", { fetcher_id });
        }
    } else {
        reset_failure_count(fetcher_id);
    }

    drop(_guard);
    tokio::time::sleep(Duration::from_secs(5)).await;
}
```

### 3.3 测试按钮触发流程

```
前端: trigger_scheduled_fetcher_test(fetcher_id)
后端: scheduler.run_now(fetcher_id) -> run_id
       (走和定时循环相同的 global_mutex + run_once)
前端: 弹窗 listen("scheduled-run-update", run_id, ...)
       根据 status 渲染："拉取列表中" → "解析中" → "写入 CSV" → "完成: 新增 X 条"
```

## 4. 容错与边界

### 4.1 yt-dlp 缺失

- 启动时探测：`Command::new_sidecar("yt-dlp").arg("--version").output()`
- 失败时 → AppState.scheduler_disabled = true
- 所有 register 调用变为 noop
- 前端通过 `get_scheduler_health` Tauri Command 获知，UI 灰显「定时任务」卡片并提示"yt-dlp 未就绪"

### 4.2 输出目录不存在

- 写 CSV 前 `fs::create_dir_all(output_dir)`
- 仍失败 → run 标记 Failed，error_message="无法创建输出目录: {path}: {err}"

### 4.3 yt-dlp 输出非预期格式

- `parse_entries` 严格按 `\t` split 8 列；行数不足 8 → 忽略该行 + warn 日志
- 0 行有效 entry 但 exit_code=0 → run.fetched_total=0, status=Success（不算失败）

### 4.4 时间窗口边界检测

- `matched.len() == 1000` 即"取完了所有 1000 条都还在窗口内" → run.progress_message 加 warn："样本可能不完整，时间窗口超出 1000 条样本范围"
- UI 在 ScheduledRunsList 中显示黄色 badge

### 4.5 同时配置多个定时任务且全部到期

- 全局 `Mutex` 保证一次只跑一个
- 队列顺序按 spawn 顺序（启动早的先抢锁）
- 后到的等待，不会丢

### 4.6 用户在执行中编辑/删除配置

- save_configs 时 scheduler 重新构建调度图
- 正在跑的 run 不会中断（已持有 mutex），但下一轮不再被调度
- 删除 fetcher 时，对应内存的 `run_counter` / `consecutive_failures` 状态一并清理；输出目录下的历史 CSV 由用户自行管理（不删除）

## 5. 性能预算

| 操作 | 预算 | 实测 / 估算 | 备注 |
|---|---|---|---|
| yt-dlp `max_fetch_num=1000` | < 5 min | 实测 ~174s | 与 max_fetch_num 近似线性 |
| yt-dlp `max_fetch_num=200` | < 1 min | 估算 ~35s | |
| yt-dlp `max_fetch_num=5000` | < 20 min | 估算 ~14 min | 用户调到此量级时前端弹警告 |
| 解析 N 行 \t-split | < 100ms (N=1000) | 字符串解析 | |
| 单次执行内 dedup_within_run | < 50ms | HashSet O(n) | |
| 排序 + 时间窗口过滤 + 命中截断 | < 100ms | O(n log n)，n=max_fetch_num | |
| CSV 全量写入 N 行（含 BOM+表头） | < 100ms | I/O，无随机读 | 无需扫描已有文件 |
| 单次 run 总耗时（max_fetch_num=1000, num_meet=100） | < 5 min | ~3 min | |

## 6. 安全与权限

- yt-dlp sidecar 只允许 `shell:allow-execute` 范围内调用
- target_url 必须以 `https://www.tiktok.com` 或 `https://tiktok.com` 或 `https://m.tiktok.com` 开头（白名单），避免被注入恶意 URL 引导 yt-dlp 拉非预期站点
- `--extractor-args` 中的 IID 校验：仅允许数字字符（regex `^\d{10,30}$`），避免 shell 注入

## 7. 测试策略

### 7.1 Rust 单元测试（必写）

- `scheduled_fetcher::parse_entries` —— 各种 yt-dlp 输出格式（含 NA、缺失字段、含特殊字符）
- `scheduled_fetcher::dedup_within_run` —— 同 id 多次出现仅保留第一条；空集；全唯一
- `scheduled_fetcher::filter_in_window_and_truncate` —— 时间窗口边界、空集、全在窗内、首条不在窗内、命中数 > num_meet_condition 截断为最新 N 条
- `scheduled_fetcher::write_csv_new_file` —— 新建文件（BOM+表头+数据）、特殊字符转义、目录不存在自动创建、同秒冲突追加 uuid 后缀
- `scheduled_fetcher::build_csv_filename` —— `<run_index>-<YYYYMMDD_HHMMSS>.csv` 格式
- `scheduled_fetcher::validate_target_url` —— 域名白名单
- `scheduled_fetcher::validate_iid_format` —— 数字校验
- `scheduled_fetcher::failure_state_machine` —— 0→1→2 仅累加；3 设 cooldown；6 关停；任意成功 reset
- `config::VideoConfig` 反序列化兼容老 JSON（无 scheduled_fetchers 字段；scheduled_fetchers 中无 max_fetch_num/num_meet_condition/tiktok_iid/cooldown_until 字段）

### 7.2 手动 E2E（mac 本地）

1. 用真实 TikTok 音乐页 URL 配置一个 fetcher（窗口 7 天，频率 1 小时，max_fetch_num=200，num_meet_condition=10）
2. 点击"测试" → 验证 CSV 生成命名形如 `1-20260531_153012.csv`、行数 ≤ 10、字段正确
3. 再点一次"测试" → 验证生成 `2-...csv` 独立文件；同一 video_id 在两份文件中允许重复出现
4. 重启 App → 验证调度自动恢复；run_index 从 1 重新计数（CSV 命名不冲突，因时间戳不同）
5. 在主页右栏点"按配置筛选" → 验证下拉过滤；点击"查看 CSV"按钮 → Finder 打开输出目录
6. 编辑 fetcher，填错的 IID（如 `0000000000`）→ 触发失败 → 第 3 次冷却 Toast → 第 6 次彻底关停 Toast

### 7.3 跨平台 CI

- macOS：用 `yt-dlp_macos` 二进制实测一次（在 PR build 中）
- Windows：暂不在 CI 跑实链路（避免外部依赖不稳定影响 CI），仅验证 sidecar 注册不报错

## 8. 上线节奏

- 实现完毕后**直接进入 v1.0.6**（基线版本未发布前不做灰度，与现有发布节奏一致）
- 在 README/QUICKSTART 中加一节"定时元数据采集（CronFetcher）"使用说明
- 不打 feature flag，默认对所有用户启用（场景为"无配置则不影响主流程"）
