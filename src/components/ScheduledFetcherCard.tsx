import React, { useState, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ScheduledFetcher } from '../types';
import { ScheduledFetcherForm } from './ScheduledFetcherForm';
import { ScheduledFetcherTestModal } from './ScheduledFetcherTestModal';

interface Props {
  configName: string;
  fetchers: ScheduledFetcher[];
  /** 整体替换 fetchers 列表，由父组件持久化到 VideoConfig.scheduled_fetchers */
  onChange: (next: ScheduledFetcher[]) => void;
}

/**
 * 定时元数据采集任务管理卡片
 * - 列表展示已配置 fetchers + 状态徽章 + 操作按钮（编辑、删除、测试、启用开关）
 * - 内嵌"添加"按钮 → 展开表单
 * - 任务命名规则：<configName> CronFetcher #N（N 为已存在同前缀任务的最大序号 +1，删除不回收）
 */
export const ScheduledFetcherCard: React.FC<Props> = ({ configName, fetchers, onChange }) => {
  const [editing, setEditing] = useState<ScheduledFetcher | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [testingFetcher, setTestingFetcher] = useState<ScheduledFetcher | null>(null);

  /** 计算下一个可用的 CronFetcher #N 序号（删除不回收） */
  const nextIndex = useMemo(() => {
    const prefix = `${configName} CronFetcher #`;
    let maxN = 0;
    for (const f of fetchers) {
      if (f.name.startsWith(prefix)) {
        const tail = f.name.slice(prefix.length);
        const n = parseInt(tail, 10);
        if (!Number.isNaN(n) && n > maxN) {
          maxN = n;
        }
      }
    }
    return maxN + 1;
  }, [configName, fetchers]);

  const handleAdd = () => {
    setEditing(null);
    setShowForm(true);
  };

  const handleEdit = (fetcher: ScheduledFetcher) => {
    setEditing(fetcher);
    setShowForm(true);
  };

  const handleDelete = (fetcher: ScheduledFetcher) => {
    if (!confirm(`确定要删除「${fetcher.name}」吗？已生成的 CSV 不会被删除。`)) {
      return;
    }
    onChange(fetchers.filter((f) => f.id !== fetcher.id));
  };

  const handleToggleEnabled = (fetcher: ScheduledFetcher) => {
    const next: ScheduledFetcher = {
      ...fetcher,
      enabled: !fetcher.enabled,
      // 重新启用时清空冷却与失败计数，便于立即恢复调度
      cooldown_until: !fetcher.enabled ? null : fetcher.cooldown_until,
      consecutive_failures: !fetcher.enabled ? 0 : fetcher.consecutive_failures,
      updated_at: new Date().toISOString(),
    };
    onChange(fetchers.map((f) => (f.id === fetcher.id ? next : f)));
  };

  const handleSaveForm = (saved: ScheduledFetcher) => {
    const exists = fetchers.some((f) => f.id === saved.id);
    if (exists) {
      onChange(fetchers.map((f) => (f.id === saved.id ? saved : f)));
    } else {
      onChange([...fetchers, saved]);
    }
    setShowForm(false);
    setEditing(null);
  };

  const handleTest = async (fetcher: ScheduledFetcher) => {
    try {
      // 调用后端命令触发 manual run（健康检查 + run_now）
      await invoke<string>('trigger_scheduled_fetcher_test', { fetcherId: fetcher.id });
      setTestingFetcher(fetcher);
    } catch (e) {
      alert(`测试失败：${e}`);
    }
  };

  /** 渲染状态徽章（cooldown / disabled / normal） */
  const renderBadge = (f: ScheduledFetcher) => {
    if (!f.enabled) {
      return (
        <span className="px-2 py-0.5 text-xs bg-gray-200 text-gray-700 rounded">已停用</span>
      );
    }
    if (f.cooldown_until) {
      const until = new Date(f.cooldown_until);
      const now = new Date();
      if (until.getTime() > now.getTime()) {
        return (
          <span className="px-2 py-0.5 text-xs bg-yellow-100 text-yellow-800 rounded" title={until.toLocaleString()}>
            冷却中（至 {until.toLocaleTimeString()}）
          </span>
        );
      }
    }
    if (f.consecutive_failures > 0) {
      return (
        <span className="px-2 py-0.5 text-xs bg-orange-100 text-orange-800 rounded">
          连续失败 {f.consecutive_failures} 次
        </span>
      );
    }
    return (
      <span className="px-2 py-0.5 text-xs bg-green-100 text-green-800 rounded">运行中</span>
    );
  };

  /** 拼接「每 X 天 Y 小时」可读频率 */
  const formatInterval = (f: ScheduledFetcher) => {
    const parts: string[] = [];
    if (f.interval_days > 0) parts.push(`${f.interval_days} 天`);
    if (f.interval_hours > 0) parts.push(`${f.interval_hours} 小时`);
    return parts.length > 0 ? `每 ${parts.join(' ')}` : '每 1 小时';
  };

  /** 拼接「最近 X 天 Y 小时」窗口 */
  const formatWindow = (f: ScheduledFetcher) => {
    const parts: string[] = [];
    if (f.window_days > 0) parts.push(`${f.window_days} 天`);
    if (f.window_hours > 0) parts.push(`${f.window_hours} 小时`);
    return parts.length > 0 ? `最近 ${parts.join(' ')}` : '最近 1 小时';
  };

  if (showForm) {
    return (
      <ScheduledFetcherForm
        configName={configName}
        fetcher={editing}
        suggestedName={editing ? editing.name : `${configName} CronFetcher #${nextIndex}`}
        onSave={handleSaveForm}
        onCancel={() => {
          setShowForm(false);
          setEditing(null);
        }}
      />
    );
  }

  return (
    <div className="border border-gray-200 rounded-xl p-5 bg-gradient-to-br from-blue-50/40 to-purple-50/40">
      <div className="flex items-center justify-between mb-3">
        <div>
          <h4 className="text-base font-semibold text-gray-800 flex items-center gap-2">
            <span>⏰</span>
            <span>定时元数据采集</span>
          </h4>
          <p className="text-xs text-gray-500 mt-1">
            定期从 TikTok 链接（音乐/Tag/用户主页）采集最新视频元数据并写入 CSV
          </p>
        </div>
        <button
          onClick={handleAdd}
          className="px-4 py-2 bg-primary text-white rounded-lg text-sm hover:bg-primary/90 transition-colors flex items-center gap-1"
        >
          <span>＋</span>
          <span>添加任务</span>
        </button>
      </div>

      {fetchers.length === 0 ? (
        <div className="text-center py-8 text-sm text-gray-400">
          暂无定时任务，点击右上角「添加任务」创建
        </div>
      ) : (
        <div className="space-y-2">
          {fetchers.map((f) => (
            <div
              key={f.id}
              className="bg-white border border-gray-200 rounded-lg px-4 py-3 flex items-center gap-3 hover:shadow-sm transition-shadow"
            >
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-1">
                  <span className="text-sm font-medium text-gray-800 truncate">{f.name}</span>
                  {renderBadge(f)}
                </div>
                <div className="text-xs text-gray-500 truncate" title={f.target_url}>
                  🔗 {f.target_url || '<未设置链接>'}
                </div>
                <div className="text-xs text-gray-400 mt-0.5">
                  {formatInterval(f)} · {formatWindow(f)} · 拉取 {f.max_fetch_num} / 取 {f.num_meet_condition}
                </div>
              </div>
              <div className="flex items-center gap-1 flex-shrink-0">
                <label className="inline-flex items-center cursor-pointer mr-1" title={f.enabled ? '点击停用' : '点击启用'}>
                  <input
                    type="checkbox"
                    className="sr-only peer"
                    checked={f.enabled}
                    onChange={() => handleToggleEnabled(f)}
                  />
                  <div className="relative w-9 h-5 bg-gray-300 peer-checked:bg-green-500 rounded-full transition-colors after:content-[''] after:absolute after:top-0.5 after:left-0.5 after:bg-white after:rounded-full after:h-4 after:w-4 after:transition-transform peer-checked:after:translate-x-4"></div>
                </label>
                <button
                  onClick={() => handleTest(f)}
                  className="px-2 py-1 text-xs text-blue-600 hover:bg-blue-50 rounded"
                  title="立即测试一次"
                >
                  测试
                </button>
                <button
                  onClick={() => handleEdit(f)}
                  className="px-2 py-1 text-xs text-gray-600 hover:bg-gray-100 rounded"
                >
                  编辑
                </button>
                <button
                  onClick={() => handleDelete(f)}
                  className="px-2 py-1 text-xs text-red-600 hover:bg-red-50 rounded"
                >
                  删除
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {testingFetcher && (
        <ScheduledFetcherTestModal
          fetcher={testingFetcher}
          onClose={() => setTestingFetcher(null)}
        />
      )}
    </div>
  );
};
