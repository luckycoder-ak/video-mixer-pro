import React, { useState, useMemo } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { ScheduledFetcher } from '../types';

const isTauriEnv = typeof window !== 'undefined' && (window as any).__TAURI__ !== undefined;

interface Props {
  configName: string;
  fetcher: ScheduledFetcher | null;
  /** 推荐的默认任务名（新建时由 Card 计算 <configName> CronFetcher #N） */
  suggestedName: string;
  onSave: (saved: ScheduledFetcher) => void;
  onCancel: () => void;
}

/**
 * 生成 UUID（fallback：crypto.randomUUID 不可用时退回伪 UUID）
 */
const genId = (): string => {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return 'fid-' + Date.now().toString(16) + '-' + Math.random().toString(16).slice(2, 10);
};

/**
 * 创建空白 fetcher 表单数据
 */
const createDefaultFetcher = (suggestedName: string): ScheduledFetcher => ({
  id: genId(),
  name: suggestedName,
  target_url: '',
  window_days: 1,
  window_hours: 0,
  interval_days: 0,
  interval_hours: 1,
  output_dir: '',
  max_fetch_num: 1000,
  num_meet_condition: 100,
  tiktok_iid: '',
  enabled: true,
  consecutive_failures: 0,
  cooldown_until: null,
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
});

/**
 * 定时采集任务编辑表单
 * - 校验：URL 非空 / 窗口与频率非负且 ≥ 1 小时 / max_fetch_num & num_meet_condition ≥ 1
 * - tiktok_iid 折叠在「高级选项」中
 */
export const ScheduledFetcherForm: React.FC<Props> = ({ fetcher, suggestedName, onSave, onCancel }) => {
  const [form, setForm] = useState<ScheduledFetcher>(fetcher || createDefaultFetcher(suggestedName));
  const [showAdvanced, setShowAdvanced] = useState(!!form.tiktok_iid);
  const [error, setError] = useState<string | null>(null);

  /** 更新单字段 */
  const upd = <K extends keyof ScheduledFetcher>(key: K, value: ScheduledFetcher[K]) => {
    setForm((prev) => ({ ...prev, [key]: value }));
  };

  /** 选择输出目录（CSV 持久化目录） */
  const handleSelectDir = async () => {
    if (!isTauriEnv) {
      const fake = window.prompt('请输入输出目录路径', form.output_dir);
      if (fake) upd('output_dir', fake);
      return;
    }
    const selected = await open({ directory: true, multiple: false });
    if (selected && typeof selected === 'string') {
      upd('output_dir', selected);
    }
  };

  /** 间隔总小时数（用于校验最小 1 小时） */
  const totalIntervalHours = useMemo(
    () => form.interval_days * 24 + form.interval_hours,
    [form.interval_days, form.interval_hours],
  );
  /** 窗口总小时数 */
  const totalWindowHours = useMemo(
    () => form.window_days * 24 + form.window_hours,
    [form.window_days, form.window_hours],
  );

  const handleSubmit = () => {
    setError(null);
    const trimmedUrl = form.target_url.trim();
    if (!trimmedUrl) {
      setError('请填写 TikTok 目标链接');
      return;
    }
    if (!form.name.trim()) {
      setError('请填写任务名称');
      return;
    }
    if (totalIntervalHours < 1) {
      setError('执行频率不能小于 1 小时');
      return;
    }
    if (totalWindowHours < 1) {
      setError('时间窗口不能小于 1 小时');
      return;
    }
    if (form.max_fetch_num < 1) {
      setError('单次拉取条数（max_fetch_num）必须 ≥ 1');
      return;
    }
    if (form.num_meet_condition < 1) {
      setError('满足条件取数（num_meet_condition）必须 ≥ 1');
      return;
    }
    if (!form.output_dir.trim()) {
      setError('请选择输出目录');
      return;
    }

    onSave({
      ...form,
      target_url: trimmedUrl,
      name: form.name.trim(),
      output_dir: form.output_dir.trim(),
      tiktok_iid: form.tiktok_iid.trim(),
      updated_at: new Date().toISOString(),
    });
  };

  return (
    <div className="border border-gray-200 rounded-xl p-5 bg-white">
      <div className="flex items-center justify-between mb-4">
        <h4 className="text-base font-semibold text-gray-800 flex items-center gap-2">
          <span>{fetcher ? '✏️' : '＋'}</span>
          <span>{fetcher ? '编辑定时任务' : '新建定时任务'}</span>
        </h4>
      </div>

      <div className="space-y-4">
        {/* 任务名称 */}
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-1">任务名称</label>
          <input
            type="text"
            value={form.name}
            onChange={(e) => upd('name', e.target.value)}
            className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
            placeholder={suggestedName}
          />
          <p className="text-xs text-gray-400 mt-1">
            建议遵循默认命名规则「&lt;配置名&gt; CronFetcher #N」便于识别
          </p>
        </div>

        {/* 目标 URL */}
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-1">TikTok 目标链接</label>
          <input
            type="text"
            value={form.target_url}
            onChange={(e) => upd('target_url', e.target.value)}
            className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
            placeholder="https://www.tiktok.com/music/xxxx 或 https://www.tiktok.com/@user 等"
          />
        </div>

        {/* 时间窗口 */}
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-1">采集时间窗口</label>
          <div className="flex items-center gap-3">
            <span className="text-xs text-gray-500">最近</span>
            <input
              type="number"
              min={0}
              value={form.window_days}
              onChange={(e) => upd('window_days', Math.max(0, parseInt(e.target.value || '0', 10)))}
              className="w-20 px-2 py-1.5 border border-gray-300 rounded-lg text-sm"
            />
            <span className="text-xs text-gray-500">天</span>
            <input
              type="number"
              min={0}
              value={form.window_hours}
              onChange={(e) => upd('window_hours', Math.max(0, parseInt(e.target.value || '0', 10)))}
              className="w-20 px-2 py-1.5 border border-gray-300 rounded-lg text-sm"
            />
            <span className="text-xs text-gray-500">小时</span>
            <span className="text-xs text-gray-400 ml-auto">总计 {totalWindowHours} 小时</span>
          </div>
        </div>

        {/* 执行频率 */}
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-1">执行频率</label>
          <div className="flex items-center gap-3">
            <span className="text-xs text-gray-500">每</span>
            <input
              type="number"
              min={0}
              value={form.interval_days}
              onChange={(e) => upd('interval_days', Math.max(0, parseInt(e.target.value || '0', 10)))}
              className="w-20 px-2 py-1.5 border border-gray-300 rounded-lg text-sm"
            />
            <span className="text-xs text-gray-500">天</span>
            <input
              type="number"
              min={0}
              value={form.interval_hours}
              onChange={(e) => upd('interval_hours', Math.max(0, parseInt(e.target.value || '0', 10)))}
              className="w-20 px-2 py-1.5 border border-gray-300 rounded-lg text-sm"
            />
            <span className="text-xs text-gray-500">小时</span>
            <span className="text-xs text-gray-400 ml-auto">最小 1 小时</span>
          </div>
        </div>

        {/* 拉取参数 */}
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">单次拉取条数</label>
            <input
              type="number"
              min={1}
              value={form.max_fetch_num}
              onChange={(e) => upd('max_fetch_num', Math.max(1, parseInt(e.target.value || '1', 10)))}
              className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm"
            />
            <p className="text-xs text-gray-400 mt-1">默认 1000，越大覆盖越广耗时越长</p>
          </div>
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">满足条件取数</label>
            <input
              type="number"
              min={1}
              value={form.num_meet_condition}
              onChange={(e) => upd('num_meet_condition', Math.max(1, parseInt(e.target.value || '1', 10)))}
              className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm"
            />
            <p className="text-xs text-gray-400 mt-1">默认 100，写入 CSV 的最大行数</p>
          </div>
        </div>

        {/* 输出目录 */}
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-1">输出目录</label>
          <div className="flex gap-3">
            <button
              onClick={handleSelectDir}
              className="px-4 py-2 border border-gray-300 rounded-lg hover:bg-gray-50 text-sm"
            >
              选择文件夹
            </button>
            <div className="flex-1 px-3 py-2 bg-gray-50 border border-gray-200 rounded-lg text-sm text-gray-600 flex items-center gap-2">
              <span>📁</span>
              <span className="truncate">{form.output_dir || '请选择输出目录...'}</span>
            </div>
          </div>
        </div>

        {/* 高级选项（IID） */}
        <div className="border-t border-gray-200 pt-3">
          <button
            onClick={() => setShowAdvanced((v) => !v)}
            className="text-sm text-gray-600 hover:text-gray-900 flex items-center gap-1"
          >
            <span>{showAdvanced ? '▼' : '▶'}</span>
            <span>高级选项</span>
          </button>
          {showAdvanced && (
            <div className="mt-3 space-y-2">
              <label className="block text-sm font-medium text-gray-700">tiktok_iid（可选）</label>
              <input
                type="text"
                value={form.tiktok_iid}
                onChange={(e) => upd('tiktok_iid', e.target.value)}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm font-mono"
                placeholder="为空则使用内置默认 IID"
              />
              <p className="text-xs text-gray-400">
                若 yt-dlp TikTok 提取器报错 missing app_info，可在此填入有效 IID 覆盖默认值
              </p>
            </div>
          )}
        </div>

        {error && (
          <div className="px-3 py-2 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700">
            ⚠ {error}
          </div>
        )}

        <div className="flex justify-end gap-2 pt-2 border-t border-gray-200">
          <button
            onClick={onCancel}
            className="px-4 py-2 border border-gray-300 rounded-lg text-sm hover:bg-gray-50"
          >
            取消
          </button>
          <button
            onClick={handleSubmit}
            className="px-4 py-2 bg-primary text-white rounded-lg text-sm hover:bg-primary/90"
          >
            保存
          </button>
        </div>
      </div>
    </div>
  );
};
