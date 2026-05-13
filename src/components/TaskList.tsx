import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Task, TaskStep } from '../types';
import { DeleteConfirmModal } from './DeleteConfirmModal';

interface Props {
  tasks: Task[];
  onRefresh: () => void;
}

export const TaskList: React.FC<Props> = ({ tasks, onRefresh }) => {
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; task: Task } | null>(null);
  const [hoveredTask, setHoveredTask] = useState<Task | null>(null);
  const [hoverPosition, setHoverPosition] = useState<{ x: number; y: number } | null>(null);
  const [hoverTimeout, setHoverTimeout] = useState<number | null>(null);
  const [deleteModalTask, setDeleteModalTask] = useState<Task | null>(null);
  const [selectedTasks, setSelectedTasks] = useState<Set<string>>(new Set());
  const [selectAll, setSelectAll] = useState(false);

  useEffect(() => {
    const interval = setInterval(() => {
      onRefresh();
    }, 1000);

    return () => clearInterval(interval);
  }, [onRefresh]);

  useEffect(() => {
    return () => {
      if (hoverTimeout) {
        clearTimeout(hoverTimeout);
      }
    };
  }, [hoverTimeout]);

  const handlePauseTask = async (task: Task) => {
    try {
      await invoke('pause_task', { id: task.id });
      setContextMenu(null);
      onRefresh();
    } catch (error) {
      console.error('暂停任务失败:', error);
    }
  };

  const handleResumeTask = async (task: Task) => {
    try {
      await invoke('resume_task', { id: task.id });
      setContextMenu(null);
      onRefresh();
    } catch (error) {
      console.error('恢复任务失败:', error);
    }
  };

  const handleDeleteTask = (task: Task) => {
    setContextMenu(null);
    setTimeout(() => {
      setDeleteModalTask(task);
    }, 50);
  };

  const handleDeleteConfirm = async (task: Task, deleteVideos: boolean) => {
    try {
      await invoke('delete_task', { id: task.id, deleteVideos });
      onRefresh();
      setDeleteModalTask(null);
    } catch (error) {
      console.error('删除任务失败:', error);
      alert(`删除任务失败: ${error}`);
    }
  };

  const handleRetryTask = async (task: Task) => {
    try {
      await invoke('retry_task', { id: task.id });
      setContextMenu(null);
      onRefresh();
    } catch (error) {
      console.error('重试任务失败:', error);
      alert(`重试任务失败: ${error}`);
    }
  };

  const toggleTaskSelection = (taskId: string) => {
    const newSelected = new Set(selectedTasks);
    if (newSelected.has(taskId)) {
      newSelected.delete(taskId);
    } else {
      newSelected.add(taskId);
    }
    setSelectedTasks(newSelected);
    setSelectAll(newSelected.size === tasks.length);
  };

  const toggleSelectAll = () => {
    if (selectAll) {
      setSelectedTasks(new Set());
    } else {
      setSelectedTasks(new Set(tasks.map(t => t.id)));
    }
    setSelectAll(!selectAll);
  };

  const handleBatchDelete = async (deleteVideos: boolean) => {
    if (selectedTasks.size === 0) {
      alert('请先选择要删除的任务');
      return;
    }

    const confirmed = window.confirm(`确定要删除选中的 ${selectedTasks.size} 个任务吗？${deleteVideos ? '同时将删除这些任务生成的视频文件。' : ''}`);

    if (!confirmed) return;

    try {
      for (const taskId of selectedTasks) {
        await invoke('delete_task', { id: taskId, deleteVideos });
      }
      setSelectedTasks(new Set());
      setSelectAll(false);
      onRefresh();
    } catch (error) {
      console.error('批量删除任务失败:', error);
      alert(`批量删除任务失败: ${error}`);
    }
  };

  const handleOpenFolder = async (task: Task) => {
    try {
      const outputPath = task.output_folder || '';
      if (outputPath) {
        await invoke('open_folder', { path: outputPath });
      } else {
        alert('该任务没有输出路径信息');
      }
    } catch (error) {
      console.error('打开文件夹失败:', error);
    }
    setContextMenu(null);
  };

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
      case 'partial':
        return (
          <span className="px-3 py-1.5 rounded-full text-xs font-semibold bg-orange-100 text-orange-800 flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-orange-500" />
            部分完成
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

  const calculateExecutionTime = (task: Task): { totalSeconds: number; averageSeconds: number } => {
    if (!task.started_at) {
      return { totalSeconds: 0, averageSeconds: 0 };
    }

    const startTime = new Date(task.started_at);
    const endTime = task.completed_at ? new Date(task.completed_at) : new Date();

    const totalMs = endTime.getTime() - startTime.getTime();
    const totalSeconds = Math.round(totalMs / 1000);

    const completedCount = task.completed_count || 0;
    const averageSeconds = completedCount > 0 ? Math.round(totalSeconds / completedCount) : 0;

    return { totalSeconds, averageSeconds };
  };

  const getStepOrder = (id: string): number => {
    const orderMap: Record<string, number> = {
      'init': 0,
      'finish': 9999,
    };
    if (orderMap[id] !== undefined) return orderMap[id];
    
    if (id.startsWith('segment_')) {
      const parts = id.split('_');
      const videoIndex = parseInt(parts[1], 10);
      const subStep = parts[2];
      const subOrderMap: Record<string, number> = {
        'scan': 0,
        'process': 1,
        'merge': 2
      };
      return videoIndex * 100 + (subOrderMap[subStep] || 0);
    }
    
    if (id.startsWith('video_')) {
      const parts = id.split('_');
      const videoIndex = parseInt(parts[1], 10);
      return videoIndex * 100 - 1;
    }
    
    return 999;
  };

  const handleContextMenu = (e: React.MouseEvent, task: Task) => {
    e.preventDefault();
    setContextMenu({ x: e.pageX, y: e.pageY, task });
    setHoveredTask(null);
  };

  const handleMouseEnter = (e: React.MouseEvent, task: Task) => {
    if (task.status === 'running' || task.status === 'paused' || task.status === 'completed' || task.status === 'error' || task.status === 'partial') {
      const rect = e.currentTarget.getBoundingClientRect();
      const popupHeight = 400;
      const windowHeight = window.innerHeight;
      const bottomSpace = windowHeight - rect.bottom;
      
      let popupY = rect.top;
      if (bottomSpace < popupHeight && rect.top > popupHeight) {
        popupY = rect.top - popupHeight;
      }
      
      const newPosition = { x: rect.left + 10, y: popupY };
      
      const timeout = window.setTimeout(() => {
        setHoverPosition(newPosition);
        setHoveredTask(task);
      }, 2000);
      
      setHoverTimeout(timeout);
    }
  };

  const handleMouseLeave = () => {
    if (hoverTimeout) {
      clearTimeout(hoverTimeout);
      setHoverTimeout(null);
    }
    setHoveredTask(null);
    setHoverPosition(null);
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
        <div className="flex justify-between items-center p-4 bg-gray-50 border-b border-gray-200">
          <div className="flex items-center gap-4">
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={selectAll && tasks.length > 0}
                onChange={toggleSelectAll}
                className="w-4 h-4 text-primary rounded border-gray-300 focus:ring-primary"
              />
              <span className="text-xs font-semibold text-gray-500 uppercase tracking-wide">全选</span>
            </label>
            {selectedTasks.size > 0 && (
              <div className="flex items-center gap-2">
                <span className="text-sm text-gray-600">已选择 {selectedTasks.size} 项</span>
                <button
                  onClick={() => handleBatchDelete(false)}
                  className="px-3 py-1.5 text-xs font-medium text-white bg-red-500 rounded-lg hover:bg-red-600 transition-colors"
                >
                  批量删除
                </button>
                <button
                  onClick={() => handleBatchDelete(true)}
                  className="px-3 py-1.5 text-xs font-medium text-white bg-red-600 rounded-lg hover:bg-red-700 transition-colors"
                >
                  删除并清理视频
                </button>
              </div>
            )}
          </div>
        </div>

        <div className="overflow-x-auto">
          <div className="min-w-[1000px]">
            <div className="grid grid-cols-15 gap-4 p-4 bg-gray-50 border-b border-gray-200 text-xs font-semibold text-gray-500 uppercase tracking-wide">
              <div className="col-span-1"></div>
              <div className="col-span-1">序号</div>
              <div className="col-span-3">任务名称</div>
              <div className="col-span-2">创建时间</div>
              <div className="col-span-2">状态</div>
              <div className="col-span-3">进度</div>
              <div className="col-span-1">耗时</div>
              <div className="col-span-2">操作</div>
            </div>

            {tasks.length === 0 ? (
              <div className="p-12 text-center text-gray-500">
                <div className="text-6xl mb-4 opacity-50">📋</div>
                <p className="text-lg mb-2">暂无任务</p>
                <p className="text-sm text-gray-400">在配置列表中点击"生成"按钮创建任务</p>
              </div>
            ) : (
              [...tasks].sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime()).map((task, index) => {
                const { totalSeconds } = calculateExecutionTime(task);
                return (
                  <div
                    key={task.id}
                    className={`grid grid-cols-15 gap-4 p-4 border-b border-gray-100 hover:bg-gray-50 transition-colors items-center cursor-pointer group ${
                      selectedTasks.has(task.id) ? 'bg-blue-50' : ''
                    }`}
                    onContextMenu={(e) => handleContextMenu(e, task)}
                    onMouseEnter={(e) => handleMouseEnter(e, task)}
                    onMouseLeave={handleMouseLeave}
                  >
                    <div className="col-span-1">
                      <input
                        type="checkbox"
                        checked={selectedTasks.has(task.id)}
                        onChange={() => toggleTaskSelection(task.id)}
                        onClick={(e) => e.stopPropagation()}
                        className="w-4 h-4 text-primary rounded border-gray-300 focus:ring-primary"
                      />
                    </div>
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
                    <div className="col-span-2">
                      {getStatusBadge(task.status)}
                      {(task.status === 'error' || task.status === 'partial') && task.error_message && (
                        <div
                          className="mt-1 text-xs text-red-600 break-all whitespace-pre-wrap leading-tight max-h-20 overflow-y-auto"
                          title={task.error_message}
                        >
                          {task.error_message}
                        </div>
                      )}
                      {task.status === 'partial' && task.failed_count > 0 && (
                        <p className="text-xs text-orange-600 mt-0.5">
                          失败 {task.failed_count} 个
                        </p>
                      )}
                    </div>
                    <div className="col-span-3">
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
                    <div className="col-span-1">
                      <span className="text-xs text-gray-500">
                        {totalSeconds}s
                      </span>
                    </div>
                    <div className="col-span-2">
                      <div className="flex gap-2">
                        {task.output_folder && task.output_folder.trim() !== '' && (
                          <button
                            onClick={() => handleOpenFolder(task)}
                            className="px-3 py-1.5 text-sm border border-gray-300 text-gray-700 rounded-lg hover:bg-gray-100 transition-colors"
                          >
                            打开文件夹
                          </button>
                        )}
                        {task.status === 'paused' ? (
                          <button
                            onClick={() => handleResumeTask(task)}
                            className="px-3 py-1.5 text-sm bg-gradient-to-r from-green-500 to-green-600 text-white rounded-lg hover:shadow-md transition-colors"
                          >
                            继续
                          </button>
                        ) : task.status === 'running' ? (
                          <button
                            onClick={() => handlePauseTask(task)}
                            className="px-3 py-1.5 text-sm bg-gradient-to-r from-yellow-500 to-yellow-600 text-white rounded-lg hover:shadow-md transition-colors"
                          >
                            暂停
                          </button>
                        ) : (task.status === 'error' || task.status === 'partial') ? (
                          <button
                            onClick={() => handleRetryTask(task)}
                            className="px-3 py-1.5 text-sm bg-gradient-to-r from-blue-500 to-blue-600 text-white rounded-lg hover:shadow-md transition-colors"
                          >
                            重试
                          </button>
                        ) : null}
                        <button
                          onClick={() => handleDeleteTask(task)}
                          className="px-3 py-1.5 text-sm bg-red-500 text-white rounded-lg hover:bg-red-600 transition-colors"
                          title="删除任务"
                        >
                          🗑️
                        </button>
                      </div>
                    </div>
                  </div>
                );
              })
            )}
          </div>
        </div>
      </div>

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
            {contextMenu.task.status === 'paused' && (
              <button 
                onClick={() => handleResumeTask(contextMenu.task)}
                className="w-full px-4 py-2 text-left text-sm text-gray-700 hover:bg-gray-50 flex items-center gap-2"
              >
                <span>▶️</span>
                <span>继续执行</span>
              </button>
            )}
            {contextMenu.task.status === 'running' && (
              <button
                onClick={() => handlePauseTask(contextMenu.task)}
                className="w-full px-4 py-2 text-left text-sm text-gray-700 hover:bg-gray-50 flex items-center gap-2"
              >
                <span>⏸️</span>
                <span>暂停任务</span>
              </button>
            )}
            {(contextMenu.task.status === 'error' || contextMenu.task.status === 'partial') && (
              <button
                onClick={() => handleRetryTask(contextMenu.task)}
                className="w-full px-4 py-2 text-left text-sm text-blue-600 hover:bg-blue-50 flex items-center gap-2"
              >
                <span>🔄</span>
                <span>重试任务</span>
              </button>
            )}
            <div className="h-px bg-gray-200 my-2" />
            <button 
              onClick={() => handleOpenFolder(contextMenu.task)}
              className="w-full px-4 py-2 text-left text-sm text-gray-700 hover:bg-gray-50 flex items-center gap-2"
            >
              <span>📂</span>
              <span>打开输出文件夹</span>
            </button>
            <div className="h-px bg-gray-200 my-2" />
            <button 
              onClick={() => handleDeleteTask(contextMenu.task)}
              className="w-full px-4 py-2 text-left text-sm text-red-600 hover:bg-red-50 flex items-center gap-2"
            >
              <span>🗑️</span>
              <span>删除任务</span>
            </button>
          </div>
        </>
      )}

      {hoveredTask && hoverPosition && (
        <div
          className="fixed z-50 bg-white rounded-xl shadow-2xl w-80 max-h-[60vh] overflow-hidden animate-fadeIn"
          style={{ left: hoverPosition.x, top: hoverPosition.y }}
        >
          <div className="bg-gradient-to-r from-gray-900 to-gray-800 px-4 py-3">
            <h3 className="text-white text-sm font-semibold flex items-center gap-2">
              <span>📊</span>
              <span>任务进度</span>
            </h3>
            <p className="text-gray-400 text-xs mt-1 truncate">{hoveredTask.task_name}</p>
          </div>

          <div className="px-4 py-3 border-b border-gray-200">
            <div className="flex items-center justify-between mb-2">
              <span className="text-gray-600 text-xs">进度</span>
              <span className="font-semibold text-gray-800 text-sm">
                {hoveredTask.completed_count}/{hoveredTask.total_count}
              </span>
            </div>
            <div className="h-2 bg-gray-200 rounded-full overflow-hidden">
              <div
                className="h-full bg-gradient-to-r from-primary to-primary-light rounded-full transition-all duration-300"
                style={{ width: `${(hoveredTask.completed_count / hoveredTask.total_count) * 100}%` }}
              />
            </div>
            {hoveredTask.current_video > 0 && (
              <p className="text-gray-500 text-xs mt-2">
                当前: 第 {hoveredTask.current_video} 个视频
              </p>
            )}
          </div>

          <div className="p-3 overflow-y-auto max-h-[30vh]">
            <h4 className="text-gray-800 font-semibold text-xs mb-3 flex items-center gap-1">
              <span>✅</span>
              <span>步骤清单</span>
            </h4>
            
            {hoveredTask.progress_steps && hoveredTask.progress_steps.length > 0 ? (
              <div className="space-y-2">
                {[...hoveredTask.progress_steps]
                  .filter(step => !step.id.startsWith('video_'))
                  .filter(step => {
                    if (step.id === 'init' || step.id === 'finish') return true;
                    
                    if (step.id.startsWith('segment_')) {
                      const parts = step.id.split('_');
                      const videoIndex = parseInt(parts[1], 10);
                      return videoIndex === hoveredTask.current_video;
                    }
                    
                    return true;
                  })
                  .sort((a, b) => getStepOrder(a.id) - getStepOrder(b.id))
                  .map((step) => {
                    const isSubStep = step.id.startsWith('segment_');
                    return (
                      <div
                        key={step.id}
                        className={`flex items-start gap-2 rounded-lg border text-xs ${getStepStatusColor(step.status)} ${isSubStep ? 'py-2 px-3' : 'p-2'}`}
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
                            <p className="text-red-500 text-xs mt-1 truncate">
                              {step.error}
                            </p>
                          )}
                        </div>
                      </div>
                    );
                  })}
                
                {hoveredTask.completed_count > 0 || hoveredTask.total_count > 0 ? (
                  <div className="mt-2 pt-2 border-t border-gray-200 text-xs text-gray-500">
                    已完成: {hoveredTask.completed_count}/{hoveredTask.total_count} 个视频
                  </div>
                ) : null}

                {hoveredTask.failed_videos && hoveredTask.failed_videos.length > 0 && (
                  <div className="mt-2 pt-2 border-t border-gray-200">
                    <h4 className="text-red-700 font-semibold text-xs mb-2 flex items-center gap-1">
                      <span>❌</span>
                      <span>失败清单（{hoveredTask.failed_videos.length}）</span>
                    </h4>
                    <div className="space-y-1 max-h-40 overflow-y-auto">
                      {hoveredTask.failed_videos.map((msg, idx) => (
                        <div
                          key={idx}
                          className="text-xs text-red-600 bg-red-50 border border-red-100 rounded px-2 py-1 break-all whitespace-pre-wrap"
                        >
                          {msg}
                        </div>
                      ))}
                    </div>
                  </div>
                )}

                {hoveredTask.error_message && (!hoveredTask.failed_videos || hoveredTask.failed_videos.length === 0) && (
                  <div className="mt-2 pt-2 border-t border-gray-200">
                    <div className="text-xs text-red-600 bg-red-50 border border-red-100 rounded px-2 py-1 break-all whitespace-pre-wrap">
                      {hoveredTask.error_message}
                    </div>
                  </div>
                )}
              </div>
            ) : (
              <div className="text-center text-gray-500 py-4">
                <div className="text-2xl mb-1">⏳</div>
                <p className="text-xs">正在初始化...</p>
              </div>
            )}
          </div>
        </div>
      )}

      {deleteModalTask && (
        <DeleteConfirmModal
            key={deleteModalTask.id}
            task={deleteModalTask}
            onConfirm={handleDeleteConfirm}
            onCancel={() => {
              setDeleteModalTask(null);
            }}
          />
      )}
    </div>
  );
};
