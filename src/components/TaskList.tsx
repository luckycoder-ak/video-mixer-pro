import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { LogEntry, Task, TaskStep } from '../types';
import { DeleteConfirmModal } from './DeleteConfirmModal';

interface Props {
  tasks: Task[];
  onRefresh: () => void;
}

export const TaskList: React.FC<Props> = ({ tasks, onRefresh }) => {
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; task: Task } | null>(null);
  const [detailTask, setDetailTask] = useState<Task | null>(null);
  const [detailTab, setDetailTab] = useState<string>('');
  const [deleteModalTask, setDeleteModalTask] = useState<Task | null>(null);
  const [selectedTasks, setSelectedTasks] = useState<Set<string>>(new Set());
  const [selectAll, setSelectAll] = useState(false);

  useEffect(() => {
    const interval = setInterval(() => {
      onRefresh();
    }, 3000);

    return () => clearInterval(interval);
  }, [onRefresh]);

  useEffect(() => {
    if (!detailTask) return;
    const latestTask = tasks.find((task) => task.id === detailTask.id);
    if (latestTask) {
      setDetailTask(latestTask);
    }
  }, [tasks, detailTask]);

  useEffect(() => {
    if (detailTask) {
      const tabs = getDetailTabs(detailTask);
      if (!detailTab || !tabs.some((tab) => tab.key === detailTab)) {
        setDetailTab(tabs[0]?.key || '');
      }
    }
  }, [detailTask, detailTab]);

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

  const getTaskStartTime = (task: Task) => {
    if (task.started_at) {
      return new Date(task.started_at);
    }

    const stepStartTimes = (task.progress_steps || [])
      .map((step) => step.started_at)
      .filter((value): value is string => Boolean(value))
      .map((value) => new Date(value))
      .filter((date) => !Number.isNaN(date.getTime()))
      .sort((a, b) => a.getTime() - b.getTime());

    if (stepStartTimes.length > 0) {
      return stepStartTimes[0];
    }

    if (task.created_at) {
      const createdAt = new Date(task.created_at);
      if (!Number.isNaN(createdAt.getTime())) {
        return createdAt;
      }
    }

    return null;
  };

  const calculateExecutionTime = (task: Task): { totalSeconds: number; averageSeconds: number } => {
    const startTime = getTaskStartTime(task);
    if (!startTime) {
      return { totalSeconds: 0, averageSeconds: 0 };
    }

    const endTime = task.completed_at ? new Date(task.completed_at) : new Date();

    const totalMs = endTime.getTime() - startTime.getTime();
    const totalSeconds = Math.round(totalMs / 1000);

    const completedCount = task.completed_count || 0;
    const averageSeconds = completedCount > 0 ? Math.round(totalSeconds / completedCount) : 0;

    return { totalSeconds, averageSeconds };
  };

  const formatSeconds = (seconds: number) => {
    if (seconds < 60) return `${seconds}s`;
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}m ${secs}s`;
  };

  const calculateStepDuration = (step: TaskStep) => {
    if (!step.started_at) return null;
    const start = new Date(step.started_at).getTime();
    const end = step.completed_at ? new Date(step.completed_at).getTime() : Date.now();
    return Math.max(0, Math.round((end - start) / 1000));
  };

  const getCurrentRunningStep = (task: Task) => {
    const runningSteps = (task.progress_steps || []).filter((step) => step.status === 'running');
    return runningSteps[runningSteps.length - 1] || null;
  };

  const getStepVideoIndex = (step: TaskStep) => {
    const match = step.id.match(/^video_(\d+)(?:__.*)?$/) || step.id.match(/^segment_(\d+)__/);
    return match ? parseInt(match[1], 10) : 0;
  };

  const getVisibleSteps = (task: Task, tab: string = '') => {
    return [...(task.progress_steps || [])]
      .filter(step => !/^video_\d+$/.test(step.id))
      .filter(step => {
        const stepVideoIndex = getStepVideoIndex(step);
        if (tab.startsWith('video-')) {
          const targetVideo = parseInt(tab.replace('video-', ''), 10);
          return stepVideoIndex === targetVideo;
        }
        return true;
      })
      .sort((a, b) => getStepOrder(a.id) - getStepOrder(b.id));
  };

  const getDetailTabs = (task: Task) => {
    const videoIndexes = new Set<number>();
    (task.logs || []).forEach((log) => {
      if (log.video_index > 0) {
        videoIndexes.add(log.video_index);
      }
    });
    (task.progress_steps || []).forEach((step) => {
      const stepVideoIndex = getStepVideoIndex(step);
      if (stepVideoIndex > 0) {
        videoIndexes.add(stepVideoIndex);
      }
    });

    return [...videoIndexes].sort((a, b) => a - b).map((videoIndex) => ({
      key: `video-${videoIndex}`,
      label: `视频${videoIndex}`,
    }));
  };

  const getFilteredLogs = (task: Task, tab: string = '') => {
    const logs = [...(task.logs || [])].sort(
      (a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime()
    );
    if (tab.startsWith('video-')) {
      const targetVideo = parseInt(tab.replace('video-', ''), 10);
      return logs.filter((log) => log.video_index === targetVideo);
    }
    return logs;
  };

  const getLogLevelColor = (level: LogEntry['level']) => {
    switch (level) {
      case 'error':
        return 'text-red-700 bg-red-50 border-red-200';
      case 'warn':
        return 'text-yellow-700 bg-yellow-50 border-yellow-200';
      default:
        return 'text-gray-700 bg-gray-50 border-gray-200';
    }
  };

  const getStepOrder = (id: string): number => {
    const orderMap: Record<string, number> = {
      'init': 0,
      'finish': 9999,
    };
    if (orderMap[id] !== undefined) return orderMap[id];

    const match = id.match(/^video_(\d+)(?:__(.+))?$/);
    if (match) {
      const videoIndex = parseInt(match[1], 10);
      const subStep = match[2] || '';
      const subOrderMap: Record<string, number> = {
        '': 0,
        'scan_1': 10,
        'scan_2': 20,
        'scan_3': 30,
        'scan_4': 40,
        'segment_1': 50,
        'segment_2': 60,
        'segment_3': 70,
        'segment_4': 80,
        'tutorial': 90,
        'xfade': 100,
        'concat': 110,
        'supplement': 120,
        'audio': 130,
        'subtitle': 140,
      };
      return videoIndex * 1000 + (subOrderMap[subStep] ?? 900);
    }
    
    return 999;
  };

  const handleContextMenu = (e: React.MouseEvent, task: Task) => {
    e.preventDefault();
    setContextMenu({ x: e.pageX, y: e.pageY, task });
    setDetailTask(null);
  };

  const openDetailTask = (task: Task) => {
    const tabs = getDetailTabs(task);
    setDetailTab(tabs[0]?.key || '');
    setDetailTask(task);
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
                      {getCurrentRunningStep(task) && (
                        <div className="mt-1 text-xs text-blue-700 truncate" title={getCurrentRunningStep(task)?.name}>
                          当前步骤: {getCurrentRunningStep(task)?.name}
                        </div>
                      )}
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
                        <button
                          onClick={() => openDetailTask(task)}
                          className="px-3 py-1.5 text-sm border border-blue-300 text-blue-700 rounded-lg hover:bg-blue-50 transition-colors"
                        >
                          详情
                        </button>
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

      {detailTask && (
        <>
          <div className="fixed inset-0 z-40 bg-black/30" onClick={() => setDetailTask(null)} />
          <div className="fixed inset-0 z-50 overflow-y-auto p-6">
            <div className="mx-auto my-4 flex w-full max-w-6xl flex-col overflow-hidden rounded-2xl border border-gray-200 bg-white shadow-2xl animate-fadeIn max-h-[calc(100vh-32px)]">
              <div className="flex items-start justify-between px-6 py-4 bg-gradient-to-r from-gray-900 to-gray-800">
                <div>
                  <h3 className="text-white text-lg font-semibold flex items-center gap-2">
                    <span>📊</span>
                    <span>任务详情</span>
                  </h3>
                  <p className="text-gray-300 text-sm mt-1 break-all">{detailTask.task_name}</p>
                </div>
                <button
                  onClick={() => setDetailTask(null)}
                  className="text-white/80 hover:text-white text-xl leading-none"
                >
                  ×
                </button>
              </div>

              <div className="grid grid-cols-3 gap-4 px-6 py-4 border-b border-gray-200 bg-gray-50">
                <div className="rounded-xl bg-white border border-gray-200 p-4">
                  <div className="text-xs text-gray-500 mb-1">执行进度</div>
                  <div className="text-lg font-semibold text-gray-800">
                    {detailTask.completed_count}/{detailTask.total_count}
                  </div>
                  <div className="mt-2 h-2 bg-gray-200 rounded-full overflow-hidden">
                    <div
                      className="h-full bg-gradient-to-r from-primary to-primary-light rounded-full transition-all"
                      style={{ width: `${(detailTask.completed_count / detailTask.total_count) * 100}%` }}
                    />
                  </div>
                </div>
                <div className="rounded-xl bg-white border border-gray-200 p-4">
                  <div className="text-xs text-gray-500 mb-1">当前步骤</div>
                  <div className="text-sm font-semibold text-blue-700 break-all">
                    {getCurrentRunningStep(detailTask)?.name || '暂无运行中的步骤'}
                  </div>
                  <div className="text-xs text-gray-500 mt-2">
                    当前视频: {detailTask.current_video || '-'}
                  </div>
                </div>
                <div className="rounded-xl bg-white border border-gray-200 p-4">
                  <div className="text-xs text-gray-500 mb-1">累计耗时</div>
                  <div className="text-lg font-semibold text-gray-800">
                    {formatSeconds(calculateExecutionTime(detailTask).totalSeconds)}
                  </div>
                  <div className="text-xs text-gray-500 mt-2">
                    平均每个视频 {formatSeconds(calculateExecutionTime(detailTask).averageSeconds)}
                  </div>
                </div>
              </div>

              {getDetailTabs(detailTask).length > 0 && (
                <div className="px-6 py-3 border-b border-gray-200 bg-white">
                  <div className="flex flex-wrap gap-2">
                    {getDetailTabs(detailTask).map((tab) => (
                      <button
                        key={tab.key}
                        onClick={() => setDetailTab(tab.key)}
                        className={`px-3 py-1.5 rounded-lg text-sm border transition-colors ${
                          detailTab === tab.key
                            ? 'bg-blue-50 border-blue-300 text-blue-700'
                            : 'bg-gray-50 border-gray-200 text-gray-600 hover:bg-gray-100'
                        }`}
                      >
                        {tab.label}
                      </button>
                    ))}
                  </div>
                </div>
              )}

              <div className="grid min-h-0 flex-1 grid-cols-2 gap-0">
                <div className="border-r border-gray-200 overflow-y-auto p-6 pb-10">
                  <div className="flex items-center justify-between mb-4">
                    <h4 className="text-base font-semibold text-gray-800">步骤清单</h4>
                    <span className="text-xs text-gray-500">{getVisibleSteps(detailTask, detailTab).length} 个步骤</span>
                  </div>
                  {getVisibleSteps(detailTask, detailTab).length > 0 ? (
                    <div className="space-y-3">
                      {getVisibleSteps(detailTask, detailTab).map((step) => {
                        const duration = calculateStepDuration(step);
                        const isSubStep = step.id.startsWith('segment_');
                        return (
                          <div
                            key={step.id}
                            className={`rounded-xl border ${getStepStatusColor(step.status)} ${isSubStep ? 'px-4 py-3' : 'p-4'}`}
                          >
                            <div className="flex items-start gap-3">
                              <div className="mt-0.5">{getStepIcon(step.status)}</div>
                              <div className="flex-1 min-w-0">
                                <div className="flex items-center justify-between gap-3">
                                  <p className={`font-medium ${
                                    step.status === 'completed' ? 'text-green-700' :
                                    step.status === 'running' ? 'text-blue-700' :
                                    step.status === 'error' ? 'text-red-700' : 'text-gray-600'
                                  }`}>
                                    {step.name}
                                  </p>
                                  <div className="text-xs text-gray-500 whitespace-nowrap">
                                    {duration !== null ? `耗时 ${formatSeconds(duration)}` : '未开始'}
                                  </div>
                                </div>
                                <div className="mt-1 text-xs text-gray-500 flex flex-wrap gap-3">
                                  <span>开始: {step.started_at ? formatDate(step.started_at) : '-'}</span>
                                  <span>结束: {step.completed_at ? formatDate(step.completed_at) : '-'}</span>
                                </div>
                                {step.error && (
                                  <div className="mt-2 text-xs text-red-600 bg-red-50 border border-red-100 rounded-lg px-3 py-2 break-all whitespace-pre-wrap">
                                    {step.error}
                                  </div>
                                )}
                              </div>
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  ) : (
                    <div className="text-center text-gray-500 py-10">
                      <div className="text-3xl mb-2">⏳</div>
                      <p className="text-sm">正在初始化步骤...</p>
                    </div>
                  )}
                </div>

                <div className="overflow-y-auto p-6 pb-10">
                  <div className="flex items-center justify-between mb-4">
                    <h4 className="text-base font-semibold text-gray-800">执行日志</h4>
                    <span className="text-xs text-gray-500">{getFilteredLogs(detailTask, detailTab).length} 条</span>
                  </div>

                  {getFilteredLogs(detailTask, detailTab).length > 0 ? (
                    <div className="space-y-2">
                      {getFilteredLogs(detailTask, detailTab).map((log, idx) => (
                        <div
                          key={`${log.timestamp}-${idx}`}
                          className={`rounded-xl border px-3 py-3 text-sm ${getLogLevelColor(log.level)}`}
                        >
                          <div className="flex items-center justify-between gap-3 text-xs">
                            <span className="font-semibold uppercase">{log.level}</span>
                            <span>{formatDate(log.timestamp)}</span>
                          </div>
                          {log.video_index > 0 && (
                            <div className="mt-1 text-xs text-gray-500">关联视频: 第 {log.video_index} 个</div>
                          )}
                          <div className="mt-2 break-all whitespace-pre-wrap leading-6">{log.message}</div>
                        </div>
                      ))}
                    </div>
                  ) : (
                    <div className="text-center text-gray-500 py-10">
                      <div className="text-3xl mb-2">📝</div>
                      <p className="text-sm">暂无日志输出</p>
                    </div>
                  )}

                  {detailTask.failed_videos && detailTask.failed_videos.length > 0 && (
                    <div className="mt-6 pt-6 border-t border-gray-200">
                      <h4 className="text-red-700 font-semibold text-sm mb-3">失败清单（{detailTask.failed_videos.length}）</h4>
                      <div className="space-y-2">
                        {detailTask.failed_videos.map((msg, idx) => (
                          <div
                            key={idx}
                            className="text-xs text-red-600 bg-red-50 border border-red-100 rounded-lg px-3 py-2 break-all whitespace-pre-wrap"
                          >
                            {msg}
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              </div>
            </div>
          </div>
        </>
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
