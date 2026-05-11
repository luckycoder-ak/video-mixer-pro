export interface TemplateSegment {
  segment_index: number;
  source_folder: string;
  crop_mode: 'single' | 'dual' | 'quadrant';
  duration: number;
}

export interface VideoConfig {
  id: string;
  name: string;
  video_ratio: string;
  audio_path: string;
  audio_duration: number;
  template_duration: number;
  segment_count: number;
  template_segments: TemplateSegment[];
  tutorial_folder: string;
  output_folder: string;
  created_at: string;
  updated_at: string;
}

export interface TaskStep {
  id: string;
  name: string;
  status: 'pending' | 'running' | 'completed' | 'error';
  error?: string;
}

export interface Task {
  id: string;
  config_id: string;
  config_name: string;
  task_name: string;
  total_count: number;
  completed_count: number;
  status: 'pending' | 'running' | 'completed' | 'paused' | 'error';
  output_folder: string;
  created_at: string;
  started_at: string | null;
  completed_at: string | null;
  error_message: string | null;
  current_video: number;
  progress_steps: TaskStep[];
}

export const createDefaultConfig = (): VideoConfig => ({
  id: '',
  name: '',
  video_ratio: '9:16',
  audio_path: '',
  audio_duration: 0,
  template_duration: 150,
  segment_count: 3,
  template_segments: [
    { segment_index: 1, source_folder: '', crop_mode: 'single', duration: 50 },
    { segment_index: 2, source_folder: '', crop_mode: 'single', duration: 50 },
    { segment_index: 3, source_folder: '', crop_mode: 'single', duration: 50 },
  ],
  tutorial_folder: '',
  output_folder: '',
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
});
