import React, { useState, useCallback } from 'react';
import { Task } from '../types';

interface Props {
  task: Task;
  onConfirm: (task: Task, deleteVideos: boolean) => void;
  onCancel: () => void;
}

export const DeleteConfirmModal: React.FC<Props> = ({ task, onConfirm, onCancel }) => {
  const [deleteVideos, setDeleteVideos] = useState(false);
  const hasOutput = task.output_folder && task.output_folder.trim() !== '';

  const handleConfirm = useCallback(() => {
    // 直接将 task 对象传递给 onConfirm，避免闭包问题
    onConfirm(task, deleteVideos);
  }, [task, deleteVideos, onConfirm]);

  const handleCancel = useCallback(() => {
    onCancel();
  }, [onCancel]);

  return (
    <div 
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
      onClick={handleCancel}
    >
      <div 
        className="bg-white rounded-2xl shadow-2xl w-full max-w-md mx-4 overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="bg-gradient-to-r from-red-500 to-red-600 px-6 py-4">
          <h3 className="text-white text-lg font-semibold">确认删除任务</h3>
        </div>

        {/* Content */}
        <div className="p-6">
          <div className="flex items-start gap-4">
            <div className="w-12 h-12 bg-red-100 rounded-full flex items-center justify-center flex-shrink-0">
              <span className="text-red-500 text-2xl">⚠️</span>
            </div>
            <div className="flex-1">
              <p className="text-gray-700 mb-2">
                确定要删除任务 <span className="font-semibold text-gray-900">"{task.task_name}"</span> 吗？
              </p>
              <p className="text-sm text-gray-500">此操作将删除任务信息，无法撤销。</p>
            </div>
          </div>

          {/* Delete Videos Checkbox */}
          {hasOutput && (
            <div className="mt-6 p-4 bg-amber-50 rounded-xl border border-amber-200">
              <label className="flex items-center gap-3 cursor-pointer">
                <input
                  type="checkbox"
                  checked={deleteVideos}
                  onChange={(e) => setDeleteVideos(e.target.checked)}
                  className="w-5 h-5 text-red-600 rounded border-gray-300 focus:ring-red-500"
                />
                <div>
                  <p className="text-sm font-medium text-gray-800">删除该任务生成的视频</p>
                  <p className="text-xs text-gray-500 mt-1">
                    将删除输出文件夹: {task.output_folder}
                  </p>
                </div>
              </label>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="px-6 py-4 bg-gray-50 flex justify-end gap-3">
          <button
            onClick={handleCancel}
            className="px-5 py-2.5 text-sm font-medium text-gray-600 bg-white border border-gray-300 rounded-xl hover:bg-gray-50 transition-colors"
          >
            取消
          </button>
          
          <button
            onClick={handleConfirm}
            className="px-5 py-2.5 text-sm font-medium text-white bg-red-500 rounded-xl hover:bg-red-600 transition-colors"
          >
            确认删除
          </button>
        </div>
      </div>
    </div>
  );
};