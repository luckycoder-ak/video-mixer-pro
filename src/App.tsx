import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ConfigList } from './components/ConfigList';
import { TaskList } from './components/TaskList';
import { ConfigModal } from './components/ConfigModal';
import { GenerateModal } from './components/GenerateModal';
import { Notification } from './components/Notification';
import { VideoConfig, Task } from './types';

function App() {
  const [activeTab, setActiveTab] = useState<'configs' | 'tasks'>('configs');
  const [configs, setConfigs] = useState<VideoConfig[]>([]);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [showConfigModal, setShowConfigModal] = useState(false);
  const [showGenerateModal, setShowGenerateModal] = useState(false);
  const [editingConfig, setEditingConfig] = useState<VideoConfig | null>(null);
  const [generatingConfig, setGeneratingConfig] = useState<VideoConfig | null>(null);
  const [notification, setNotification] = useState<{ show: boolean; title: string; message: string }>({
    show: false,
    title: '',
    message: '',
  });

  useEffect(() => {
    loadData();
  }, []);

  const loadData = async () => {
    try {
      const data = await invoke<{ configs: VideoConfig[]; tasks: Task[] }>('load_data');
      const loadedConfigs = data.configs || [];
      const loadedTasks = data.tasks || [];
      setConfigs(loadedConfigs);
      setTasks(loadedTasks);
      return { configs: loadedConfigs, tasks: loadedTasks };
    } catch (error) {
      console.error('Failed to load data:', error);
      return { configs: [], tasks: [] };
    }
  };

  const refreshTasks = async () => {
    try {
      const freshTasks = await invoke<Task[]>('get_tasks');
      const freshConfigs = await invoke<VideoConfig[]>('get_configs');
      setTasks(freshTasks);
      setConfigs(freshConfigs);
    } catch (error) {
      console.error('Failed to refresh tasks:', error);
    }
  };

  const getDataFilePath = async () => {
    try {
      return await invoke<string>('get_data_file_path');
    } catch (error) {
      console.error('Failed to get data file path:', error);
      return '';
    }
  };

  const handleSaveConfig = async (config: VideoConfig) => {
    try {
      const savedConfig = await invoke<VideoConfig>('save_config', { config });
      let newConfigs: VideoConfig[];
      const existingIndex = configs.findIndex((c) => c.id === savedConfig.id);
      if (existingIndex >= 0) {
        newConfigs = [...configs];
        newConfigs[existingIndex] = savedConfig;
      } else {
        newConfigs = [...configs, savedConfig];
      }
      setConfigs(newConfigs);
      await invoke('save_configs', { configs: newConfigs, tasks });
      const filePath = await getDataFilePath();
      setShowConfigModal(false);
      setEditingConfig(null);
      showNotification('保存成功', `配置已保存到:\n${filePath}`);
    } catch (error) {
      showNotification('保存失败', String(error));
    }
  };

  const handleGenerate = async (config: VideoConfig, count: number) => {
    try {
      const newTask = await invoke<Task>('create_task', { configName: config.name, count });
      const newTasks = [...tasks, newTask];
      setTasks(newTasks);
      await invoke('save_configs', { configs, tasks: newTasks });
      setShowGenerateModal(false);
      setGeneratingConfig(null);
      setActiveTab('tasks');
      showNotification('任务已创建', `正在为"${config.name}"生成 ${count} 个视频`);
    } catch (error) {
      showNotification('创建失败', String(error));
    }
  };

  const handleEditConfig = (config: VideoConfig) => {
    setEditingConfig(config);
    setShowConfigModal(true);
  };

  const handleNewConfig = () => {
    setEditingConfig(null);
    setShowConfigModal(true);
  };

  const showNotification = (title: string, message: string) => {
    setNotification({ show: true, title, message });
    setTimeout(() => {
      setNotification({ show: false, title: '', message: '' });
    }, 3000);
  };

  return (
    <div className="w-full max-w-7xl bg-gray-50 rounded-2xl shadow-2xl overflow-hidden animate-fadeIn">
      {/* Header */}
      <div className="bg-gradient-to-r from-gray-900 to-gray-800 px-6 py-4 flex items-center justify-between border-b border-gray-700">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 bg-gradient-to-br from-primary to-primary-dark rounded-xl flex items-center justify-center text-white font-bold text-lg shadow-lg">
            VM
          </div>
          <h1 className="text-white text-xl font-semibold">VideoMixer Pro</h1>
        </div>
      </div>

      {/* Tabs */}
      <div className="bg-white px-6 border-b border-gray-200 flex gap-1">
        <button
          onClick={() => setActiveTab('configs')}
          className={`px-6 py-4 text-sm font-medium transition-all relative ${
            activeTab === 'configs'
              ? 'text-primary'
              : 'text-gray-500 hover:text-gray-700'
          }`}
        >
          <span className="flex items-center gap-2">
            <span>⚙️</span>
            <span>配置管理</span>
          </span>
          {activeTab === 'configs' && (
            <div className="absolute bottom-0 left-0 right-0 h-0.5 bg-primary rounded-t" />
          )}
        </button>
        <button
          onClick={() => setActiveTab('tasks')}
          className={`px-6 py-4 text-sm font-medium transition-all relative ${
            activeTab === 'tasks'
              ? 'text-primary'
              : 'text-gray-500 hover:text-gray-700'
          }`}
        >
          <span className="flex items-center gap-2">
            <span>📋</span>
            <span>任务列表</span>
          </span>
          {activeTab === 'tasks' && (
            <div className="absolute bottom-0 left-0 right-0 h-0.5 bg-primary rounded-t" />
          )}
        </button>
      </div>

      {/* Content */}
      <div className="p-4 bg-gray-50">
        {activeTab === 'configs' ? (
          <ConfigList
            configs={configs}
            onNew={handleNewConfig}
            onEdit={handleEditConfig}
            onGenerate={(config) => {
              setGeneratingConfig(config);
              setShowGenerateModal(true);
            }}
            onRefresh={refreshTasks}
          />
        ) : (
          <TaskList
            tasks={tasks}
            onRefresh={refreshTasks}
          />
        )}
      </div>

      {/* Modals */}
      {showConfigModal && (
        <ConfigModal
          config={editingConfig}
          onSave={handleSaveConfig}
          onClose={() => {
            setShowConfigModal(false);
            setEditingConfig(null);
          }}
        />
      )}

      {showGenerateModal && generatingConfig && (
        <GenerateModal
          config={generatingConfig}
          onGenerate={(count) => handleGenerate(generatingConfig, count)}
          onClose={() => {
            setShowGenerateModal(false);
            setGeneratingConfig(null);
          }}
        />
      )}

      {/* Notification */}
      <Notification
        show={notification.show}
        title={notification.title}
        message={notification.message}
      />

      <style>{`
        @keyframes fadeIn {
          from { opacity: 0; transform: translateY(20px) scale(0.98); }
          to { opacity: 1; transform: translateY(0) scale(1); }
        }
        .animate-fadeIn {
          animation: fadeIn 0.6s ease-out;
        }
      `}</style>
    </div>
  );
}

export default App;
