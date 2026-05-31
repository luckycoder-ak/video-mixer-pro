import { useMemo, useState } from 'react';
import { VideoConfig, ScheduledFetcher } from '../types';
import { ScheduledFetcherCard } from './ScheduledFetcherCard';
import { ScheduledRunsList } from './ScheduledRunsList';

interface Props {
  configs: VideoConfig[];
  /** 整体替换某个 config 的 scheduled_fetchers，由父组件落盘到 save_config + save_configs */
  onUpdateFetchers: (configId: string, next: ScheduledFetcher[]) => void;
}

/**
 * 定时采集 Tab：左侧按配置分组管理采集任务，右侧展示执行历史。
 *
 * - 左：每个配置一个折叠卡片，复用 ScheduledFetcherCard
 * - 右：复用 ScheduledRunsList（跨配置聚合视图）
 */
export function ScheduledFetchersTab({ configs, onUpdateFetchers }: Props) {
  const sortedConfigs = useMemo(
    () => [...configs].sort((a, b) => a.name.localeCompare(b.name, 'zh-CN')),
    [configs],
  );
  const [expandedId, setExpandedId] = useState<string | null>(
    sortedConfigs.length > 0 ? sortedConfigs[0].id : null,
  );

  if (configs.length === 0) {
    return (
      <div className="bg-white rounded-lg shadow-sm border border-gray-200 p-8 text-center text-gray-500">
        暂无配置，请先在「配置管理」中创建至少一个视频配置。
      </div>
    );
  }

  return (
    <div className="grid grid-cols-2 gap-4 h-[calc(100vh-160px)]">
      <div className="overflow-y-auto space-y-3 pr-1">
        <div className="text-sm font-medium text-gray-600 px-1">采集任务（按配置分组）</div>
        {sortedConfigs.map((cfg) => {
          const fetchers = cfg.scheduled_fetchers || [];
          const isExpanded = expandedId === cfg.id;
          return (
            <div
              key={cfg.id}
              className="bg-white rounded-lg shadow-sm border border-gray-200 overflow-hidden"
            >
              <button
                onClick={() => setExpandedId(isExpanded ? null : cfg.id)}
                className="w-full px-4 py-3 flex items-center justify-between hover:bg-gray-50 transition-colors"
              >
                <div className="flex items-center gap-2">
                  <span className="text-base">📁</span>
                  <span className="font-medium text-gray-800">{cfg.name}</span>
                  <span className="text-xs text-gray-500">
                    {fetchers.length} 个采集任务
                  </span>
                </div>
                <span className="text-gray-400 text-sm">{isExpanded ? '▼' : '▶'}</span>
              </button>
              {isExpanded && (
                <div className="px-4 pb-4 border-t border-gray-100">
                  <ScheduledFetcherCard
                    configName={cfg.name}
                    fetchers={fetchers}
                    onChange={(next) => onUpdateFetchers(cfg.id, next)}
                  />
                </div>
              )}
            </div>
          );
        })}
      </div>
      <div className="overflow-hidden">
        <ScheduledRunsList configs={configs} />
      </div>
    </div>
  );
}
