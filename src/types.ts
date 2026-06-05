export interface TemplateSegment {
  segment_index: number;
  source_folder: string;
  source_folder2: string;
  crop_mode: 'single' | 'dual' | 'quadrant';
  duration: number;
  scale_percent: number;
}

/** 字幕样式配置 */
export interface SubtitleStyle {
  /** 字号 */
  fontsize: number;
  /** 字体颜色（FFmpeg 颜色格式：white / 0xRRGGBB） */
  fontcolor: string;
  /** 描边宽度 */
  borderw: number;
  /** 描边颜色 */
  bordercolor: string;
  /** 阴影/发光色 */
  shadowcolor: string;
  /** 阴影 X 偏移 */
  shadowx: number;
  /** 阴影 Y 偏移 */
  shadowy: number;
  /** 水平位置表达式 */
  x: string;
  /** 垂直位置表达式 */
  y: string;
  /** 行间距 */
  line_spacing: number;
  /** 启用逐字渐变 */
  enable_gradient: boolean;
  /** 渐变起始色 */
  gradient_color1: string;
  /** 渐变结束色 */
  gradient_color2: string;
}

const DEFAULT_SUBTITLE_STYLE: SubtitleStyle = {
  fontsize: 36,
  fontcolor: 'white',
  borderw: 3,
  bordercolor: 'black',
  shadowcolor: 'white',
  shadowx: 2,
  shadowy: 2,
  x: '(w-tw)/2',
  y: 'h-th-100',
  line_spacing: 8,
  enable_gradient: false,
  gradient_color1: '0xFF6B6B',
  gradient_color2: '0x4ECDC4',
};

/** 全局应用设置（高级设置 Tab 中可配置）。 */
export interface AppSettings {
  /** 飞书自定义机器人 Webhook URL；空字符串表示未配置，所有通知静默跳过。 */
  feishu_webhook_url: string;
}

export interface VideoConfig {
  id: string;
  name: string;
  root_folder: string;
  video_ratio: string;
  audio_path: string;
  audio_duration: number;
  subtitle_path: string;
  subtitle_style: SubtitleStyle;
  template_duration: number;
  segment_count: number;
  template_segments: TemplateSegment[];
  tutorial_folder: string;
  output_folder: string;
  enable_transition: boolean;
  scheduled_fetchers?: ScheduledFetcher[];
  created_at: string;
  updated_at: string;
}

/** 定时元数据采集任务（CronFetcher）。 */
export interface ScheduledFetcher {
  id: string;
  name: string;
  target_url: string;
  window_days: number;
  window_hours: number;
  interval_days: number;
  interval_hours: number;
  output_dir: string;
  max_fetch_num: number;
  num_meet_condition: number;
  tiktok_iid: string;
  enabled: boolean;
  consecutive_failures: number;
  cooldown_until?: string | null;
  created_at: string;
  updated_at: string;
}

export type RunTrigger = 'manual' | 'scheduled';
export type RunStatus =
  | 'pending'
  | 'fetching_list'
  | 'filtering'
  | 'writing'
  | 'success'
  | 'failed'
  | 'interrupted';

/** 单次执行的运行记录。 */
export interface ScheduledRun {
  id: string;
  fetcher_id: string;
  fetcher_name: string;
  config_id: string;
  config_name: string;
  run_index: number;
  trigger: RunTrigger;
  status: RunStatus;
  started_at: string;
  finished_at?: string | null;
  fetched_total?: number;
  matched_in_window?: number;
  new_appended?: number;
  csv_path?: string | null;
  error_message?: string | null;
  progress_message?: string;
}

export interface TaskStep {
  id: string;
  name: string;
  status: 'pending' | 'running' | 'completed' | 'error';
  error?: string;
  started_at?: string | null;
  completed_at?: string | null;
}

export interface LogEntry {
  timestamp: string;
  level: 'info' | 'warn' | 'error';
  video_index: number;
  message: string;
}

export interface Task {
  id: string;
  config_id: string;
  config_name: string;
  task_name: string;
  total_count: number;
  completed_count: number;
  failed_count: number;
  failed_videos: string[];
  status: 'pending' | 'running' | 'completed' | 'paused' | 'error' | 'partial';
  output_folder: string;
  created_at: string;
  started_at: string | null;
  completed_at: string | null;
  error_message: string | null;
  current_video: number;
  progress_steps: TaskStep[];
  logs: LogEntry[];
}

export const createDefaultConfig = (): VideoConfig => ({
  id: '',
  name: '',
  root_folder: '',
  video_ratio: '9:16',
  audio_path: '',
  audio_duration: 0,
  subtitle_path: '',
  subtitle_style: { ...DEFAULT_SUBTITLE_STYLE },
  template_duration: 150,
  segment_count: 3,
  template_segments: [
    { segment_index: 1, source_folder: '', source_folder2: '', crop_mode: 'single', duration: 50, scale_percent: 51 },
    { segment_index: 2, source_folder: '', source_folder2: '', crop_mode: 'single', duration: 50, scale_percent: 51 },
    { segment_index: 3, source_folder: '', source_folder2: '', crop_mode: 'single', duration: 50, scale_percent: 51 },
  ],
  tutorial_folder: '',
  output_folder: '',
  enable_transition: false,
  scheduled_fetchers: [],
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
});
