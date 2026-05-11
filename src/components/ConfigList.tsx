import React from 'react';
import { invoke } from '@tauri-apps/api/core';
import { VideoConfig } from '../types';

interface Props {
  configs: VideoConfig[];
  onNew: () => void;
  onEdit: (config: VideoConfig) => void;
  onGenerate: (config: VideoConfig) => void;
}

export const ConfigList: React.FC<Props> = ({ configs, onNew, onEdit, onGenerate }) => {
  const handleOpenFolder = async (folderPath: string) => {
    try {
      await invoke('open_folder', { path: folderPath });
    } catch (error) {
      console.error('打开文件夹失败:', error);
    }
  };

  return (
    <div>
      <div className="flex justify-between items-center mb-5">
        <h2 className="text-xl font-semibold text-gray-800 flex items-center gap-2">
          <span>📁</span>
          <span>配置列表</span>
        </h2>
        <button
          onClick={onNew}
          className="px-5 py-2.5 bg-gradient-to-r from-primary to-primary-dark text-white rounded-lg font-medium shadow-md hover:shadow-lg hover:-translate-y-0.5 transition-all flex items-center gap-2"
        >
          <span>+</span>
          <span>新建配置</span>
        </button>
      </div>

      <div className="bg-white rounded-xl shadow-md overflow-hidden">
        {/* Header */}
        <div className="grid grid-cols-12 gap-4 p-4 bg-gray-50 border-b border-gray-200 text-xs font-semibold text-gray-500 uppercase tracking-wide">
          <div className="col-span-1">序号</div>
          <div className="col-span-3">配置名称</div>
          <div className="col-span-1">比例</div>
          <div className="col-span-1">片段数</div>
          <div className="col-span-2">模板时长</div>
          <div className="col-span-2">总时长</div>
          <div className="col-span-2">操作</div>
        </div>

        {/* Body */}
        {configs.length === 0 ? (
          <div className="p-12 text-center text-gray-500">
            <div className="text-6xl mb-4 opacity-50">📂</div>
            <p className="text-lg mb-2">暂无配置</p>
            <p className="text-sm text-gray-400">点击"新建配置"开始创建</p>
          </div>
        ) : (
          configs.map((config, index) => (
            <div
              key={config.id}
              className="grid grid-cols-12 gap-4 p-4 border-b border-gray-100 hover:bg-gray-50 transition-colors items-center"
            >
              <div className="col-span-1 text-gray-400">{String(index + 1).padStart(2, '0')}</div>
              <div className="col-span-3">
                <div className="flex items-center gap-2">
                  <span className="font-semibold text-gray-800">{config.name}</span>
                  <span className="px-2 py-0.5 bg-primary text-white text-xs font-semibold rounded uppercase">
                    {config.video_ratio}
                  </span>
                </div>
              </div>
              <div className="col-span-1 font-mono text-gray-600">{config.video_ratio}</div>
              <div className="col-span-1 text-gray-600 flex items-center gap-1">
                <span>📺</span>
                <span>{config.segment_count} 个</span>
              </div>
              <div className="col-span-2 text-gray-600 flex items-center gap-1">
                <span>⏱️</span>
                <span>{config.template_duration} 秒</span>
              </div>
              <div className="col-span-2 text-gray-600 flex items-center gap-1">
                <span>🎵</span>
                <span>{config.audio_duration} 秒</span>
              </div>
              <div className="col-span-2 flex gap-2">
                <button
                  onClick={() => onEdit(config)}
                  className="px-3 py-1.5 text-sm border border-gray-300 text-gray-700 rounded-lg hover:bg-gray-100 transition-colors"
                >
                  编辑
                </button>
                <button
                  onClick={() => handleOpenFolder(config.root_folder)}
                  className="px-3 py-1.5 text-sm border border-gray-300 text-gray-700 rounded-lg hover:bg-gray-100 transition-colors"
                  title="打开主目录"
                >
                  📂
                </button>
                <button
                  onClick={() => onGenerate(config)}
                  className="px-3 py-1.5 text-sm bg-gradient-to-r from-secondary to-secondary-dark text-white rounded-lg hover:shadow-md transition-all"
                >
                  生成
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
