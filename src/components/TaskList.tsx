import React, { useState } from 'react';
import { Task, TaskStep } from '../types';

interface Props {
  tasks: Task[];
  onRefresh: () => void;
}

export const TaskList: React.FC<Props> = ({ tasks, onRefresh }) => {
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; task: Task } | null>(null);
  const [progressModal, setProgressModal] = useState<Task | null>(null);

  const getStatusBadge = (status: Task['status']) => {
    switch (status) {
      case 'running':
        return (
          <span className="px-3 py-1.5 rounded-full text-xs font-semibold bg-blue-100 text-blue-800 flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-blue-600 animate-pulse" />
            执行中
          </span>
        );
      case 'completed':
        return (
          <span className="px-3 py-1.5 rounded-full text-xs font-semibold bg-green-100 text-green-800 flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-green-600" />
            已完成
          </span>
        );
      case 'paused':
        return (
          <span className="px-3 py-1.5 rounded-full text-xs font-semibold bg-yellow-100 text-yellow-800 flex items-center gap-2 cursor-pointer hover:bg-yellow-200 transition-colors">
            <span className="w-2 h-2 rounded-full bg-yellow-600" />
            中断
          </span>
        );
      case 'error':
        return (
          <span className="px-3 py-1.5 rounded-full text-xs font-semibold bg-red-100 text-red-800 flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-red-600" />
            错误
          </span>
        );
      default:
        return (
          <span className="px-3 py-1.5 rounded-full text-xs font-semibold bg-gray-100 text-gray-600 flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-gray-400" />
            等待中
          </span>
        );
    }
  };

  const getStepIcon = (status: TaskStep['status']) => {
    switch (status) {
      case 'running':
        return <span className="w-5 h-5 rounded-full bg-blue-500 text-white flex items-center justify-center text-xs font-bold animate-pulse">▶</span>;
      case 'completed':
        return <span className="w-5 h-5 rounded-full bg-green-500 text-white flex items-center justify-center text-xs font-bold">✓</span>;
      case 'error':
        return <span className="w-5 h-5 rounded-full bg-red-500 text-white flex items-center justify-center text-xs font-bold">✕</span>;
      default:
        return <span className="w-5 h-5 rounded-full bg-gray-300 text-gray-500 flex items-center justify-center text-xs font-bold">○</span>;
    }
  };

  const getStepStatusColor = (status: TaskStep['status']) => {
    switch (status) {
      case 'running':
        return 'bg-blue-100 border-blue-300';
      case 'completed':
        return 'bg-green-50 border-green-200';
      case 'error':
        return 'bg-red-50 border-red-200';
      default:
        return 'bg-gray-50 border-gray-200';
    }
  };

  const formatDate = (dateString: string) => {
    const date = new Date(dateString);
    return date.toLocaleString('zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
  };

  const handleContextMenu = (e: React.MouseEvent, task: Task) => {
    e.preventDefault();
    setContextMenu({ x: e.pageX, y: e.pageY, task });
  };

  const handleViewProgress = (task: Task) => {
    setProgressModal(task);
    setContextMenu(null);
  };

  return (
    <div>
      <div className="flex justify-between items-center mb-5">
        <h2 className="text-xl font-semibold text-gray-800 flex items-center gap-2">
          <span>📋</span>
          <span>任务列表</span>
        </h2>
        <button
          onClick={onRefresh}
          className="px-5 py-2.5 border border-gray-300 text-gray-700 rounded-lg font-medium hover:bg-gray-100 transition-colors flex items-center gap-2"
        >
          <span>🔄</span>
          <span>刷新</span>
        </button>
      </div>

      <div className="bg-white rounded-xl shadow-md overflow-hidden">
        {/* Header */}
        <div className="grid grid-cols-12 gap-4 p-4 bg-gray-50 border-b border-gray-200 text-xs font-semibold text-gray-500 uppercase tracking-wide">
          <div className="col-span-1">序号</div>
          <div className="col-span-3">任务名称</div>
          <div className="col-span-2">创建时间</div>
          <div className="col-span-2">状态</div>
          <div className="col-span-2">进度</div>
          <div className="col-span-2">操作</div>
        </div>

        {/* Body */}
        {tasks.length === 0 ? (
          <div className="p-12 text-center text-gray-500">
            <div className="text-6xl mb-4 opacity-50">📋</div>
            <p className="text-lg mb-2">暂无任务</p>
            <p className="text-sm text-gray-400">在配置列表中点击"生成"按钮创建任务</p>
          </div>
        ) : (
          tasks.map((task, index) => (
            <div
              key={task.id}
              className="grid grid-cols-12 gap-4 p-4 border-b border-gray-100 hover:bg-gray-50 transition-colors items-center"
              onContextMenu={(e) => handleContextMenu(e, task)}
            >
              <div className="col-span-1 text-gray-400">{String(index + 1).padStart(2, '0')}</div>
              <div className="col-span-3">
                <div className="flex items-center gap-2">
                  <span>📁</span>
                  <span className="font-semibold text-gray-800">{task.task_name}</span>
                </div>
              </div>
              <div className="col-span-2 text-gray-500 text-sm flex items-center gap-1">
                <span>🕐</span>
                <span>{formatDate(task.created_at)}</span>
              </div>
              <div className="col-span-2">{getStatusBadge(task.status)}</div>
              <div className="col-span-2">
                <div className="flex items-center gap-2">
                  <span className="font-semibold font-mono text-gray-700">
                    {task.completed_count}/{task.total_count}
                  </span>
                  <div className="flex-1 h-2 bg-gray-200 rounded-full overflow-hidden">
                    <div
                      className="h-full bg-gradient-to-r from-primary to-primary-light rounded-full transition-all"
                      style={{ width: `${(task.completed_count / task.total_count) * 100}%` }}
                    />
                  </div>
                </div>
              </div>
              <div className="col-span-2">
                <div className="flex gap-2">
                  {task.status === 'running' && (
                    <button
                      onClick={() => handleViewProgress(task)}
                      className="px-3 py-1.5 text-sm bg-gradient-to-r from-primary to-primary-dark text-white rounded-lg hover:shadow-md transition-all"
                    >
                      查看进度
                    </button>
                  )}
                  {task.status === 'completed' ? (
                    <button className="px-3 py-1.5 text-sm border border-gray-300 text-gray-700 rounded-lg hover:bg-gray-100 transition-colors">
                      打开文件夹
                    </button>
                  ) : task.status === 'paused' ? (
                    <button className="px-3 py-1.5 text-sm bg-gradient-to-r from-secondary to-secondary-dark text-white rounded-lg hover:shadow-md transition-all">
                      继续
                    </button>
                  ) : task.status !== 'running' ? (
                    <button className="px-3 py-1.5 text-sm border border-gray-300 text-gray-700 rounded-lg hover:bg-gray-100 transition-colors">
                      暂停
                    </button>
                  ) : null}
                </div>
              </div>
            </div>
          ))
        )}
      </div>

      {/* Context Menu */}
      {contextMenu && (
        <>
          <div
            className="fixed inset-0 z-40"
            onClick={() => setContextMenu(null)}
          />
          <div
            className="fixed z-50 bg-white rounded-lg shadow-lg py-2 min-w-[160px] border border-gray-200"
            style={{ left: contextMenu.x, top: contextMenu.y }}
          >
            <button 
              onClick={() => handleViewProgress(contextMenu.task)}
              className="w-full px-4 py-2 text-left text-sm text-gray-700 hover:bg-gray-50 flex items-center gap-2"
            >
              <span>👁️</span>
              <span>查看进度</span>
            </button>
            {contextMenu.task.status === 'paused' && (
              <button className="w-full px-4 py-2 text-left text-sm text-gray-700 hover:bg-gray-50 flex items-center gap-2">
                <span>▶️</span>
                <span>继续执行</span>
              </button>
            )}
            {contextMenu.task.status === 'running' && (
              <button className="w-full px-4 py-2 text-left text-sm text-gray-700 hover:bg-gray-50 flex items-center gap-2">
                <span>⏸️</span>
                <span>暂停任务</span>
              </button>
            )}
            <div className="h-px bg-gray-200 my-2" />
            <button className="w-full px-4 py-2 text-left text-sm text-gray-700 hover:bg-gray-50 flex items-center gap-2">
              <span>📂</span>
              <span>打开输出文件夹</span>
            </button>
            <div className="h-px bg-gray-200 my-2" />
            <button className="w-full px-4 py-2 text-left text-sm text-red-600 hover:bg-red-50 flex items-center gap-2">
              <span>🗑️</span>
              <span>删除任务</span>
            </button>
          </div>
        </>
      )}

      {/* Progress Modal */}
      {progressModal && (
        <>
          <div
            className="fixed inset-0 z-50 bg-black bg-opacity-50 flex items-center justify-center p-4"
            onClick={() => setProgressModal(null)}
          />
          <div
            className="fixed z-50 bg-white rounded-xl shadow-2xl w-full max-w-lg max-h-[80vh] overflow-hidden"
            onClick={(e) => e.stopPropagation()}
          >
            {/* Header */}
            <div className="bg-gradient-to-r from-gray-900 to-gray-800 px-6 py-4 flex items-center justify-between">
              <div>
                <h3 className="text-white text-lg font-semibold flex items-center gap-2">
                  <span>📊</span>
                  <span>任务进度</span>
                </h3>
                <p className="text-gray-400 text-sm mt-1">{progressModal.task_name}</p>
              </div>
              <button
                onClick={() => setProgressModal(null)}
                className="text-gray-400 hover:text-white text-2xl font-bold"
              >
                ×
              </button>
            </div>

            {/* Progress Bar */}
            <div className="px-6 py-4 border-b border-gray-200">
              <div className="flex items-center justify-between mb-2">
                <span className="text-gray-600 text-sm">整体进度</span>
                <span className="font-semibold text-gray-800">
                  {progressModal.completed_count}/{progressModal.total_count} 视频
                </span>
              </div>
              <div className="h-3 bg-gray-200 rounded-full overflow-hidden">
                <div
                  className="h-full bg-gradient-to-r from-primary to-primary-light rounded-full transition-all duration-300"
                  style={{ width: `${(progressModal.completed_count / progressModal.total_count) * 100}%` }}
                />
              </div>
              {progressModal.current_video > 0 && (
                <p className="text-gray-500 text-sm mt-3">
                  当前处理: 第 {progressModal.current_video} 个视频
                </p>
              )}
            </div>

            {/* Steps Checklist */}
            <div className="p-6 overflow-y-auto max-h-[50vh]">
              <h4 className="text-gray-800 font-semibold mb-4 flex items-center gap-2">
                <span>✅</span>
                <span>步骤清单</span>
              </h4>
              
              {progressModal.progress_steps && progressModal.progress_steps.length > 0 ? (
                <div className="space-y-3">
                  {progressModal.progress_steps.map((step) => (
                    <div
                      key={step.id}
                      className={`flex items-start gap-3 p-3 rounded-lg border ${getStepStatusColor(step.status)}`}
                    >
                      <div className="mt-0.5">{getStepIcon(step.status)}</div>
                      <div className="flex-1">
                        <p className={`font-medium ${
                          step.status === 'completed' ? 'text-green-700' :
                          step.status === 'running' ? 'text-blue-700' :
                          step.status === 'error' ? 'text-red-700' : 'text-gray-600'
                        }`}>
                          {step.name}
                        </p>
                        {step.error && (
                          <p className="text-red-500 text-sm mt-1">
                            错误: {step.error}
                          </p>
                        )}
                      </div>
                      {step.status === 'running' && (
                        <div className="w-2 h-2 rounded-full bg-blue-500 animate-bounce" />
                      )}
                    </div>
                  ))}
                </div>
              ) : (
                <div className="text-center text-gray-500 py-8">
                  <div className="text-4xl mb-2">⏳</div>
                  <p>正在初始化任务...</p>
                </div>
              )}
            </div>

            {/* Footer */}
            <div className="px-6 py-4 border-t border-gray-200 flex justify-end">
              <button
                onClick={() => setProgressModal(null)}
                className="px-6 py-2 bg-gray-100 text-gray-700 rounded-lg hover:bg-gray-200 transition-colors"
              >
                关闭
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  );
};
