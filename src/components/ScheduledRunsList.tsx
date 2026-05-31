import React, { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { ScheduledRun, VideoConfig } from '../types';

interface Props {
  /** 当前所有可见配置（用于配置筛选下拉） */
  configs: VideoConfig[];
}

/**
 * 主页右栏：定时任务执行列表
 * - 顶部多选下拉筛选可见 config_id（默认全选）
 * - 列表按 started_at 倒序展示 ScheduledRun
 * - 实时订阅 scheduled-run-update 事件刷新列表
 * - 行内操作：点击「查看 CSV」调用 open_csv_in_finder
 */
export const ScheduledRunsList: React.FC<Props> = ({ configs }) => {
  const [runs, setRuns] = useState<ScheduledRun[]>([]);
  const [selectedConfigIds, setSelectedConfigIds] = useState<Set<string>>(
    new Set(configs.map((c) => c.id)),
  );
  const [showFilter, setShowFilter] = useState(false);

  /** 加载/刷新列表 */
  const reload = async () => {
    try {
      const ids = Array.from(selectedConfigIds);
      const list = await invoke<ScheduledRun[]>('list_scheduled_runs', {
        configIds: ids.length === configs.length ? null : ids,
      });
      setRuns(list);
    } catch (e) {
      console.error('加载定时任务执行列表失败', e);
    }
  };

  useEffect(() => {
    reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedConfigIds.size, configs.length]);

  // 当外部 configs 变化时，新增配置默认勾选
  useEffect(() => {
    setSelectedConfigIds((prev) => {
      const next = new Set(prev);
      let changed = false;
      for (const c of configs) {
        if (!next.has(c.id)) {
          next.add(c.id);
          changed = true;
        }
      }
      // 移除已删除配置
      for (const id of Array.from(next)) {
        if (!configs.find((c) => c.id === id)) {
          next.delete(id);
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [configs]);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;
    (async () => {
      try {
        unlisten = await listen<ScheduledRun>('scheduled-run-update', (event) => {
          if (cancelled) return;
          const run = event.payload;
          // 按 selected 过滤
          if (selectedConfigIds.size > 0 && !selectedConfigIds.has(run.config_id)) return;
          setRuns((prev) => {
            const idx = prev.findIndex((r) => r.id === run.id);
            if (idx >= 0) {
              const next = prev.slice();
              next[idx] = run;
              return next;
            }
            // 新 run 插入到列表头部（按 started_at 倒序）
            return [run, ...prev];
          });
        });
      } catch (e) {
        console.error('监听 scheduled-run-update 失败', e);
      }
    })();
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [selectedConfigIds]);

  const toggleConfig = (id: string) => {
    setSelectedConfigIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const allSelected = selectedConfigIds.size === configs.length;
  const toggleAll = () => {
    if (allSelected) {
      setSelectedConfigIds(new Set());
    } else {
      setSelectedConfigIds(new Set(configs.map((c) => c.id)));
    }
  };

  const handleOpenCsv = async (csvPath: string) => {
    try {
      await invoke('open_csv_in_finder', { csvPath });
    } catch (e) {
      alert(`打开 CSV 失败：${e}`);
    }
  };

  /** 状态文案与样式 */
  const renderStatus = (r: ScheduledRun) => {
    const map: Record<string, { label: string; cls: string }> = {
      pending: { label: '准备中', cls: 'bg-gray-100 text-gray-700' },
      fetching_list: { label: '拉取中', cls: 'bg-blue-100 text-blue-700' },
      filtering: { label: '过滤中', cls: 'bg-blue-100 text-blue-700' },
      writing: { label: '写入中', cls: 'bg-purple-100 text-purple-700' },
      success: { label: '✓ 成功', cls: 'bg-green-100 text-green-700' },
      failed: { label: '✗ 失败', cls: 'bg-red-100 text-red-700' },
      interrupted: { label: '⏹ 中断停止', cls: 'bg-orange-100 text-orange-700' },
    };
    const conf = map[r.status] || { label: r.status, cls: 'bg-gray-100 text-gray-700' };
    return <span className={`px-2 py-0.5 rounded text-xs ${conf.cls}`}>{conf.label}</span>;
  };

  const fmtTime = (s?: string | null) => {
    if (!s) return '-';
    return new Date(s).toLocaleString();
  };

  const filteredRuns = useMemo(() => runs, [runs]);

  return (
    <div className="bg-white rounded-xl shadow-sm border border-gray-200 flex flex-col h-full">
      <div className="px-4 py-3 border-b border-gray-200 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span>⏰</span>
          <h3 className="text-base font-semibold text-gray-800">定时任务执行</h3>
          <span className="text-xs text-gray-400">共 {filteredRuns.length} 条</span>
        </div>
        <div className="relative">
          <button
            onClick={() => setShowFilter((v) => !v)}
            className="px-3 py-1.5 text-xs border border-gray-300 rounded-lg hover:bg-gray-50 flex items-center gap-1"
          >
            <span>🔽</span>
            <span>筛选配置 ({selectedConfigIds.size}/{configs.length})</span>
          </button>
          {showFilter && (
            <div className="absolute right-0 top-full mt-1 bg-white border border-gray-200 rounded-lg shadow-lg z-10 w-56 max-h-80 overflow-y-auto">
              <div className="px-3 py-2 border-b border-gray-200 flex items-center justify-between">
                <button onClick={toggleAll} className="text-xs text-blue-600 hover:underline">
                  {allSelected ? '取消全选' : '全选'}
                </button>
                <button onClick={() => setShowFilter(false)} className="text-xs text-gray-400">
                  收起
                </button>
              </div>
              {configs.length === 0 ? (
                <div className="px-3 py-4 text-xs text-gray-400 text-center">暂无配置</div>
              ) : (
                configs.map((c) => (
                  <label
                    key={c.id}
                    className="flex items-center gap-2 px-3 py-2 hover:bg-gray-50 cursor-pointer text-sm"
                  >
                    <input
                      type="checkbox"
                      checked={selectedConfigIds.has(c.id)}
                      onChange={() => toggleConfig(c.id)}
                    />
                    <span className="truncate">{c.name}</span>
                  </label>
                ))
              )}
            </div>
          )}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto">
        {filteredRuns.length === 0 ? (
          <div className="text-center py-12 text-sm text-gray-400">暂无执行记录</div>
        ) : (
          <div className="divide-y divide-gray-100">
            {filteredRuns.map((r) => (
              <div key={r.id} className="px-4 py-3 hover:bg-gray-50">
                <div className="flex items-center justify-between gap-2 mb-1">
                  <div className="flex items-center gap-2 min-w-0">
                    <span className="text-sm font-medium text-gray-800 truncate">
                      {r.fetcher_name} #{r.run_index}
                    </span>
                    {renderStatus(r)}
                    {r.trigger === 'manual' ? (
                      <span className="px-1.5 py-0.5 text-[10px] bg-blue-50 text-blue-600 rounded border border-blue-200">
                        测试
                      </span>
                    ) : (
                      <span className="px-1.5 py-0.5 text-[10px] bg-purple-50 text-purple-600 rounded border border-purple-200">
                        定时
                      </span>
                    )}
                  </div>
                  {r.csv_path && r.status === 'success' && (
                    <button
                      onClick={() => handleOpenCsv(r.csv_path!)}
                      className="text-xs text-blue-600 hover:underline flex-shrink-0"
                    >
                      查看 CSV
                    </button>
                  )}
                </div>
                <div className="text-xs text-gray-500 mb-1 truncate">
                  📋 {r.config_name}
                </div>
                <div className="flex items-center gap-3 text-xs text-gray-400 flex-wrap">
                  <span>开始 {fmtTime(r.started_at)}</span>
                  {r.finished_at && <span>结束 {fmtTime(r.finished_at)}</span>}
                  {typeof r.fetched_total === 'number' && (
                    <span>拉取 {r.fetched_total}</span>
                  )}
                  {typeof r.matched_in_window === 'number' && (
                    <span>命中 {r.matched_in_window}</span>
                  )}
                  {typeof r.new_appended === 'number' && (
                    <span>写入 {r.new_appended}</span>
                  )}
                </div>
                {r.error_message && (
                  <div className="mt-1 px-2 py-1 bg-red-50 border border-red-200 rounded text-xs text-red-700">
                    {r.error_message}
                  </div>
                )}
                {r.progress_message && r.status !== 'success' && r.status !== 'failed' && r.status !== 'interrupted' && (
                  <div className="mt-1 text-xs text-gray-500">{r.progress_message}</div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};
