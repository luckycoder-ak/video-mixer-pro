import React, { useState } from 'react';
import { VideoConfig } from '../types';

interface Props {
  config: VideoConfig;
  onGenerate: (count: number) => void;
  onClose: () => void;
}

export const GenerateModal: React.FC<Props> = ({ config, onGenerate, onClose }) => {
  const [count, setCount] = useState(5);

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
      <div className="bg-white rounded-2xl shadow-2xl w-full max-w-md animate-fadeIn">
        {/* Header */}
        <div className="bg-gradient-to-r from-gray-900 to-gray-800 px-6 py-4 flex justify-between items-center rounded-t-2xl">
          <h3 className="text-white text-lg font-semibold flex items-center gap-2">
            <span>🎬</span>
            <span>开始生成视频任务</span>
          </h3>
          <button
            onClick={onClose}
            className="w-8 h-8 bg-gray-700 text-gray-400 rounded-lg flex items-center justify-center hover:bg-gray-600 hover:text-white transition-colors"
          >
            ×
          </button>
        </div>

        {/* Body */}
        <div className="p-6">
          <div className="bg-gray-50 p-4 rounded-lg mb-6">
            <p className="text-sm text-gray-600">
              即将为配置 <strong className="text-gray-800">{config.name}</strong> 创建生成任务
            </p>
          </div>

          <div className="mb-6">
            <label className="block text-sm font-medium text-gray-700 mb-3">
              请输入生成数量 <span className="text-red-500">*</span>
            </label>
            <div className="flex items-center gap-4">
              <button
                onClick={() => setCount(Math.max(1, count - 1))}
                className="w-10 h-10 border border-gray-300 bg-white rounded-lg flex items-center justify-center text-gray-600 hover:bg-gray-50 transition-colors text-lg font-medium"
              >
                −
              </button>
              <input
                type="number"
                value={count}
                onChange={(e) => setCount(Math.min(100, Math.max(1, parseInt(e.target.value) || 1)))}
                min="1"
                max="100"
                className="flex-1 px-4 py-2.5 border border-gray-300 rounded-lg text-center font-semibold text-lg focus:ring-2 focus:ring-primary focus:border-transparent"
              />
              <button
                onClick={() => setCount(Math.min(100, count + 1))}
                className="w-10 h-10 border border-gray-300 bg-white rounded-lg flex items-center justify-center text-gray-600 hover:bg-gray-50 transition-colors text-lg font-medium"
              >
                +
              </button>
            </div>
            <p className="text-xs text-gray-400 mt-2">
              生成后将创建后台任务执行，可在任务列表中查看进度
            </p>
          </div>
        </div>

        {/* Footer */}
        <div className="px-6 py-4 bg-gray-50 border-t border-gray-200 flex justify-end gap-3 rounded-b-2xl">
          <button
            onClick={onClose}
            className="px-5 py-2.5 border border-gray-300 text-gray-700 rounded-lg hover:bg-gray-100 transition-colors"
          >
            取消
          </button>
          <button
            onClick={() => onGenerate(count)}
            className="px-5 py-2.5 bg-gradient-to-r from-secondary to-secondary-dark text-white rounded-lg hover:shadow-lg transition-all flex items-center gap-2"
          >
            <span>✓</span>
            <span>确认生成</span>
          </button>
        </div>
      </div>

      <style>{`
        @keyframes fadeIn {
          from { opacity: 0; transform: scale(0.95) translateY(10px); }
          to { opacity: 1; transform: scale(1) translateY(0); }
        }
        .animate-fadeIn {
          animation: fadeIn 0.3s ease-out;
        }
      `}</style>
    </div>
  );
};
