import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { ScheduledFetcher, ScheduledRun } from '../types';

interface Props {
  fetcher: ScheduledFetcher;
  onClose: () => void;
}

/**
 * 定时任务测试运行实时进度弹窗
 * - 订阅后端 scheduled-run-update 事件，过滤当前 fetcher_id
 * - 展示阶段进度（fetching_list / filtering / writing / success / failed）
 * - 成功后展示「在 Finder 中查看 CSV」按钮
 */
export const ScheduledFetcherTestModal: React.FC<Props> = ({ fetcher, onClose }) => {
  const [run, setRun] = useState<ScheduledRun | null>(null);
  const [triggerError, setTriggerError] = useState<string | null>(null);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    const setup = async () => {
      try {
        // 先订阅事件，再触发后端命令，避免 listener 错过早期 emit
        unlisten = await listen<ScheduledRun>('scheduled-run-update', (event) => {
          const r = event.payload;
          if (r.fetcher_id !== fetcher.id) return;
          if (cancelled) return;
          // 仅追踪 trigger=manual 的最新运行（避免被定时运行干扰）
          if (r.trigger !== 'manual') return;
          setRun(r);
        });
      } catch (e) {
        console.error('监听 scheduled-run-update 失败', e);
      }
      // listener 就绪后再触发后端命令
      try {
        await invoke<string>('trigger_scheduled_fetcher_test', { fetcherId: fetcher.id });
      } catch (e) {
        if (!cancelled) {
          setTriggerError(String(e));
        }
      }
    };
    setup();

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [fetcher.id]);

  /** 状态对应的可读文案 */
  const statusLabel = (s?: string) => {
    switch (s) {
      case 'pending':
        return '准备启动';
      case 'fetching_list':
        return '正在拉取列表';
      case 'filtering':
        return '正在过滤排序';
      case 'writing':
        return '正在写入 CSV';
      case 'success':
        return '✅ 完成';
      case 'failed':
        return '❌ 失败';
      case 'interrupted':
        return '⏹ 中断停止';
      default:
        return triggerError ? '❌ 触发失败' : '正在启动…';
    }
  };

  const isFinal =
    run?.status === 'success' ||
    run?.status === 'failed' ||
    run?.status === 'interrupted' ||
    triggerError !== null;

  const handleOpenCsv = async () => {
    if (!run?.csv_path) return;
    try {
      await invoke('open_csv_in_finder', { csvPath: run.csv_path });
    } catch (e) {
      alert(`打开 CSV 失败：${e}`);
    }
  };

  return (
    <div
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <div className="bg-white rounded-2xl shadow-2xl w-full max-w-md animate-fadeIn">
        <div className="bg-gradient-to-r from-blue-600 to-purple-600 px-5 py-3 flex justify-between items-center rounded-t-2xl">
          <h3 className="text-white text-base font-semibold flex items-center gap-2">
            <span>🧪</span>
            <span>测试运行</span>
          </h3>
          <button
            onClick={onClose}
            className="w-7 h-7 bg-white/20 text-white rounded-lg flex items-center justify-center hover:bg-white/30"
          >
            ×
          </button>
        </div>
        <div className="p-5 space-y-4">
          <div>
            <div className="text-xs text-gray-500 mb-1">任务</div>
            <div className="text-sm font-medium text-gray-800">{fetcher.name}</div>
          </div>

          <div>
            <div className="text-xs text-gray-500 mb-1">状态</div>
            <div className="text-sm font-medium text-gray-800">{statusLabel(run?.status)}</div>
            {run?.progress_message && (
              <div className="text-xs text-gray-500 mt-1">{run.progress_message}</div>
            )}
          </div>

          {run && (
            <div className="grid grid-cols-3 gap-2 text-center">
              <div className="bg-gray-50 rounded-lg p-2">
                <div className="text-xs text-gray-500">拉取</div>
                <div className="text-sm font-semibold">{run.fetched_total ?? '-'}</div>
              </div>
              <div className="bg-gray-50 rounded-lg p-2">
                <div className="text-xs text-gray-500">命中</div>
                <div className="text-sm font-semibold">{run.matched_in_window ?? '-'}</div>
              </div>
              <div className="bg-gray-50 rounded-lg p-2">
                <div className="text-xs text-gray-500">写入</div>
                <div className="text-sm font-semibold">{run.new_appended ?? '-'}</div>
              </div>
            </div>
          )}

          {run?.error_message && (
            <div className="px-3 py-2 bg-red-50 border border-red-200 rounded-lg text-xs text-red-700">
              {run.error_message}
            </div>
          )}

          {triggerError && !run && (
            <div className="px-3 py-2 bg-red-50 border border-red-200 rounded-lg text-xs text-red-700">
              {triggerError}
            </div>
          )}

          <div className="flex justify-between gap-2 pt-2 border-t border-gray-200">
            {run?.status === 'success' && run.csv_path ? (
              <button
                onClick={handleOpenCsv}
                className="px-4 py-2 bg-blue-50 text-blue-700 rounded-lg text-sm hover:bg-blue-100 flex items-center gap-1"
              >
                <span>📂</span>
                <span>查看 CSV</span>
              </button>
            ) : (
              <span />
            )}
            <button
              onClick={onClose}
              className="px-4 py-2 border border-gray-300 rounded-lg text-sm hover:bg-gray-50"
            >
              {isFinal ? '关闭' : '后台继续'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
