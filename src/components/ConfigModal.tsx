import React, { useState, useEffect } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { VideoConfig, TemplateSegment, createDefaultConfig } from '../types';

const isTauriEnv = typeof window !== 'undefined' && (window as any).__TAURI__ !== undefined;

interface Props {
  config: VideoConfig | null;
  onSave: (config: VideoConfig) => void;
  onClose: () => void;
}

type TabType = 'basic' | 'template' | 'tutorial';

export const ConfigModal: React.FC<Props> = ({ config, onSave, onClose }) => {
  const [formData, setFormData] = useState<VideoConfig>(config || createDefaultConfig());
  const [expandedSegments, setExpandedSegments] = useState<Set<number>>(new Set([1]));
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [currentTab, setCurrentTab] = useState<TabType>('basic');

  const tabs: { key: TabType; label: string; icon: string }[] = [
    { key: 'basic', label: '基础信息配置', icon: '⚙️' },
    { key: 'template', label: '模板片段配置', icon: '🎬' },
    { key: 'tutorial', label: '教程素材配置', icon: '📚' },
  ];

  const currentTabIndex = tabs.findIndex((t) => t.key === currentTab);

  useEffect(() => {
    if (config) {
      setFormData(config);
      setExpandedSegments(new Set([1]));
    } else {
      setFormData(createDefaultConfig());
      setExpandedSegments(new Set([1]));
    }
  }, [config]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        const target = e.target as HTMLElement;
        if (target.tagName !== 'INPUT' && target.tagName !== 'TEXTAREA') {
          onClose();
        }
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose]);

  const handleInputChange = (field: keyof VideoConfig, value: any) => {
    setFormData((prev) => ({ ...prev, [field]: value }));
  };

  const handleSegmentCountChange = (count: number) => {
    if (count < 1) {
      return;
    }

    const segments: TemplateSegment[] = [];
    const avgDuration = Math.floor(formData.template_duration / count);

    for (let i = 1; i <= count; i++) {
      const existing = formData.template_segments.find((s) => s.segment_index === i);
      const duration = existing?.duration || avgDuration;

      if (i === count) {
        const usedDuration = segments.reduce((sum, s) => sum + s.duration, 0);
        const remainingDuration = formData.template_duration - usedDuration;
        segments.push(
          existing || {
            segment_index: i,
            source_folder: '',
            crop_mode: 'single',
            duration: remainingDuration > 0 ? remainingDuration : duration,
          }
        );
      } else {
        segments.push(
          existing || {
            segment_index: i,
            source_folder: '',
            crop_mode: 'single',
            duration,
          }
        );
      }
    }

    handleInputChange('template_segments', segments);
    handleInputChange('segment_count', count);

    setExpandedSegments(new Set([count]));
  };

  const handleSegmentChange = (index: number, field: keyof TemplateSegment, value: any) => {
    const newSegments = [...formData.template_segments];
    newSegments[index] = { ...newSegments[index], [field]: value };
    handleInputChange('template_segments', newSegments);
  };

  const handleSelectAudio = async () => {
    if (!isTauriEnv) {
      console.warn('请在 Tauri 应用中运行此功能');
      return;
    }
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'Audio', extensions: ['mp3', 'wav', 'aac', 'm4a', 'ogg', 'flac'] }],
      });
      if (selected) {
        handleInputChange('audio_path', selected);
        try {
          const duration = await invoke<number>('get_audio_duration', { audioPath: selected });
          handleInputChange('audio_duration', duration);
        } catch {
          handleInputChange('audio_duration', 180);
        }
      }
    } catch (error) {
      console.error('选择音频文件失败:', error);
    }
  };

  const handleSelectFolder = async (isTutorialFolder: boolean, index?: number, isRootFolder?: boolean) => {
    if (!isTauriEnv) {
      console.warn('请在 Tauri 应用中运行此功能');
      return;
    }
    try {
      const defaultPath = isRootFolder ? undefined : (formData.root_folder || undefined);
      const selected = await open({ directory: true, multiple: false, defaultPath });
      if (selected) {
        if (isRootFolder) {
          handleInputChange('root_folder', selected);
        } else if (isTutorialFolder) {
          handleInputChange('tutorial_folder', selected);
        } else if (index !== undefined) {
          handleSegmentChange(index, 'source_folder', selected);
        } else {
          handleInputChange('output_folder', selected);
        }
      }
    } catch (error) {
      console.error('选择文件夹失败:', error);
    }
  };

  const toggleSegment = (index: number) => {
    setExpandedSegments((prev) => {
      const newSet = new Set(prev);
      if (newSet.has(index)) {
        newSet.delete(index);
      } else {
        newSet.add(index);
      }
      return newSet;
    });
  };

  const totalSegmentDuration = formData.template_segments.reduce((sum, s) => sum + s.duration, 0);
  const durationDiff = formData.template_duration - totalSegmentDuration;
  const isDurationValid = totalSegmentDuration === formData.template_duration;

  const validateBasicTab = (): boolean => {
    if (!formData.root_folder) {
      alert('请选择主目录');
      return false;
    }
    if (!formData.name.trim()) {
      alert('请输入配置名称');
      return false;
    }
    if (!formData.audio_path) {
      alert('请选择音频文件');
      return false;
    }
    return true;
  };

  const validateTemplateTab = (): boolean => {
    if (!isDurationValid) {
      alert(`片段时长验证失败：当前总时长 ${totalSegmentDuration} 秒，模板片段总时长 ${formData.template_duration} 秒`);
      return false;
    }
    const hasEmptyFolders = formData.template_segments.some((s) => !s.source_folder);
    if (hasEmptyFolders) {
      alert('请为所有模板片段选择来源文件夹');
      return false;
    }
    return true;
  };

  const handleNextTab = () => {
    const currentIndex = tabs.findIndex((t) => t.key === currentTab);
    if (currentIndex < tabs.length - 1) {
      setCurrentTab(tabs[currentIndex + 1].key);
    }
  };

  const handlePrevTab = () => {
    const currentIndex = tabs.findIndex((t) => t.key === currentTab);
    if (currentIndex > 0) {
      setCurrentTab(tabs[currentIndex - 1].key);
    }
  };

  const handleSubmit = async () => {
    if (!validateBasicTab()) return;
    if (!validateTemplateTab()) return;

    setIsSubmitting(true);

    try {
      const newConfig: VideoConfig = {
        ...formData,
        id: formData.id || crypto.randomUUID(),
      };
      await onSave(newConfig);
    } catch (error) {
      console.error('保存配置失败:', error);
      alert('保存失败，请重试');
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleTemplateDurationChange = (newDuration: number) => {
    if (newDuration < 0 || isNaN(newDuration)) {
      return;
    }

    handleInputChange('template_duration', newDuration);

    const segments = [...formData.template_segments];
    if (segments.length > 0 && newDuration > 0) {
      const avgDuration = Math.floor(newDuration / segments.length);
      let remainingDuration = newDuration;

      for (let i = 0; i < segments.length; i++) {
        if (i === segments.length - 1) {
          segments[i].duration = remainingDuration;
        } else {
          segments[i].duration = avgDuration;
          remainingDuration -= avgDuration;
        }
      }

      handleInputChange('template_segments', segments);
    }
  };

  const getCropModePreview = (mode: string) => {
    switch (mode) {
      case 'single':
        return (
          <div className="grid grid-cols-1 gap-1">
            <div className="aspect-[9/16] bg-gray-700 rounded flex items-center justify-center text-xs text-gray-400">
              视频1
            </div>
          </div>
        );
      case 'dual':
        return (
          <div className="flex gap-1">
            <div className="flex-1 flex flex-col gap-1">
              <div className="h-4 bg-gray-800 rounded" />
              <div className="flex-1 aspect-[9/16] bg-gray-700 rounded flex items-center justify-center text-xs text-gray-400">
                视频1
              </div>
              <div className="h-4 bg-gray-800 rounded" />
            </div>
            <div className="flex-1 flex flex-col gap-1">
              <div className="h-4 bg-gray-800 rounded" />
              <div className="flex-1 aspect-[9/16] bg-gray-700 rounded flex items-center justify-center text-xs text-gray-400">
                视频2
              </div>
              <div className="h-4 bg-gray-800 rounded" />
            </div>
          </div>
        );
      case 'quadrant':
        return (
          <div className="grid grid-cols-2 gap-1">
            <div className="aspect-[9/16] bg-gray-700 rounded flex items-center justify-center text-xs text-gray-400">
              视频1
            </div>
            <div className="aspect-[9/16] bg-gray-700 rounded flex items-center justify-center text-xs text-gray-400">
              视频2
            </div>
            <div className="aspect-[9/16] bg-gray-700 rounded flex items-center justify-center text-xs text-gray-400">
              视频3
            </div>
            <div className="aspect-[9/16] bg-gray-700 rounded flex items-center justify-center text-xs text-gray-400">
              视频4
            </div>
          </div>
        );
      default:
        return null;
    }
  };

  const getCropModeHint = (mode: string) => {
    switch (mode) {
      case 'single':
        return '单视频模式：直接截取指定时长，保持原始宽高比';
      case 'dual':
        return '双列模式：两个视频等比例缩放至宽度一半，居中放置，上下黑边使用虚化背景填充';
      case 'quadrant':
        return '四宫格模式：四个视频等比例缩放至1/4宽度，田字格排列，上下黑边使用虚化背景填充';
      default:
        return '';
    }
  };

  const renderBasicTab = () => (
    <div className="space-y-6">
      <div>
        <label className="block text-sm font-medium text-gray-700 mb-2">
          主目录 <span className="text-red-500">*</span>
        </label>
        <div className="flex gap-3">
          <button
            onClick={() => handleSelectFolder(false, undefined, true)}
            className="px-4 py-2.5 border border-gray-300 rounded-lg hover:bg-gray-50 transition-colors"
          >
            选择主目录
          </button>
          <div className="flex-1 px-4 py-2.5 bg-gray-50 border border-gray-200 rounded-lg text-sm text-gray-600 flex items-center gap-2">
            <span>📁</span>
            <span className="truncate">
              {formData.root_folder || '请选择主目录...'}
            </span>
          </div>
        </div>
        <p className="text-xs text-gray-400 mt-1">后续目录选择将基于此目录进行，确保所有素材在同一目录下</p>
      </div>

      <div className="grid grid-cols-2 gap-6">
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-2">
            配置名称 <span className="text-red-500">*</span>
          </label>
          <input
            type="text"
            value={formData.name}
            onChange={(e) => handleInputChange('name', e.target.value)}
            placeholder="请输入配置名称，要求不重复"
            className="w-full px-4 py-2.5 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent"
          />
        </div>
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-2">视频比例</label>
          <select
            value={formData.video_ratio}
            onChange={(e) => handleInputChange('video_ratio', e.target.value)}
            className="w-full px-4 py-2.5 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent"
          >
            <option value="9:16">9:16 (竖屏)</option>
            <option value="16:9">16:9 (横屏)</option>
            <option value="1:1">1:1 (方形)</option>
          </select>
        </div>
      </div>

      <div>
        <label className="block text-sm font-medium text-gray-700 mb-2">
          音频文件 <span className="text-red-500">*</span>
        </label>
        <div className="flex gap-3">
          <button
            onClick={handleSelectAudio}
            className="px-4 py-2.5 border border-gray-300 rounded-lg hover:bg-gray-50 transition-colors"
          >
            选择音频文件
          </button>
          <div className="flex-1 px-4 py-2.5 bg-gray-50 border border-gray-200 rounded-lg text-sm text-gray-600 flex items-center gap-2">
            <span>🎵</span>
            <span className="truncate">
              {formData.audio_path || '请选择音频文件...'}
            </span>
          </div>
        </div>
      </div>

      <div>
        <label className="block text-sm font-medium text-gray-700 mb-2">输出文件夹</label>
        <div className="flex gap-3">
          <button
            onClick={() => handleSelectFolder(false, undefined)}
            className="px-4 py-2.5 border border-gray-300 rounded-lg hover:bg-gray-50 transition-colors"
          >
            选择输出文件夹
          </button>
          <div className="flex-1 px-4 py-2.5 bg-gray-50 border border-gray-200 rounded-lg text-sm text-gray-600 flex items-center gap-2">
            <span>📂</span>
            <span className="truncate">
              {formData.output_folder || '未选择（默认下载目录）'}
            </span>
          </div>
        </div>
        <p className="text-xs text-gray-400 mt-1">生成的视频将保存到此文件夹，命名格式：配置名称-1.mp4, 配置名称-2.mp4...</p>
      </div>

      {formData.audio_duration > 0 && (
        <div className="p-4 bg-blue-50 rounded-lg flex items-center gap-3 text-blue-800">
          <span className="text-xl">📊</span>
          <div>
            <p className="font-medium">
              <strong>音频总时长: {formData.audio_duration} 秒</strong>
            </p>
            <p className="text-sm mt-1 opacity-80">模板片段总时长应与此一致</p>
          </div>
        </div>
      )}
    </div>
  );

  const renderTemplateTab = () => (
    <div className="space-y-6">
      <div className="grid grid-cols-2 gap-4 mb-4">
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-2">
            模板片段总时长 <span className="text-gray-400">(秒)</span>
          </label>
          <input
            type="number"
            value={formData.template_duration}
            onChange={(e) => handleTemplateDurationChange(parseInt(e.target.value) || 0)}
            min="1"
            className="w-full px-4 py-2.5 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent"
          />
        </div>
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-2">片段数量</label>
          <select
            value={formData.segment_count}
            onChange={(e) => handleSegmentCountChange(parseInt(e.target.value))}
            className="w-full px-4 py-2.5 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent"
          >
            {[1, 2, 3, 4, 5, 6, 7, 8, 9, 10].map((n) => (
              <option key={n} value={n}>
                {n} 个片段
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="space-y-4">
        {formData.template_segments.map((segment, index) => (
          <div key={segment.segment_index} className="bg-gray-50 border border-gray-200 rounded-xl overflow-hidden">
            <div
              className="px-4 py-3 bg-white border-b border-gray-200 flex justify-between items-center cursor-pointer"
              onClick={() => toggleSegment(segment.segment_index)}
            >
              <div className="flex items-center gap-3">
                <div className="w-7 h-7 bg-gradient-to-br from-primary to-primary-dark rounded-full flex items-center justify-center text-white text-sm font-semibold">
                  {segment.segment_index}
                </div>
                <span className="font-semibold text-gray-800">片段 {segment.segment_index}</span>
                <span className="text-gray-500 text-sm">({segment.duration}秒)</span>
                {segment.source_folder && (
                  <span className="px-2 py-0.5 bg-green-100 text-green-700 text-xs rounded">已设置</span>
                )}
              </div>
              <span className={`text-gray-500 transition-transform ${expandedSegments.has(segment.segment_index) ? 'rotate-180' : ''}`}>
                ▼
              </span>
            </div>

            {expandedSegments.has(segment.segment_index) && (
              <div className="p-4 space-y-4">
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-2">片段来源文件夹</label>
                  <div className="flex gap-3">
                    <button
                      onClick={() => handleSelectFolder(false, index)}
                      className="px-4 py-2 border border-gray-300 rounded-lg hover:bg-gray-50 transition-colors text-sm"
                    >
                      选择文件夹
                    </button>
                    <div className="flex-1 px-4 py-2 bg-white border border-gray-200 rounded-lg text-sm text-gray-600 flex items-center gap-2">
                      <span>📁</span>
                      <span className="truncate">
                        {segment.source_folder || '请选择文件夹...'}
                      </span>
                    </div>
                  </div>
                </div>

                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-2">裁剪模式</label>
                  <div className="grid grid-cols-3 gap-3 mb-3">
                    {(['single', 'dual', 'quadrant'] as const).map((mode) => (
                      <div
                        key={mode}
                        onClick={() => handleSegmentChange(index, 'crop_mode', mode)}
                        className={`p-3 border-2 rounded-xl cursor-pointer transition-all text-center ${
                          segment.crop_mode === mode
                            ? 'border-primary bg-primary/5'
                            : 'border-gray-200 hover:border-gray-300'
                        }`}
                      >
                        <div className="text-2xl mb-1">
                          {mode === 'single' ? '📹' : mode === 'dual' ? '🖼️' : '📱'}
                        </div>
                        <div className="text-xs font-medium text-gray-700">
                          {mode === 'single' ? '单视频' : mode === 'dual' ? '双列' : '四宫格'}
                        </div>
                      </div>
                    ))}
                  </div>

                  <div className="bg-gray-900 rounded-lg p-3">
                    <p className="text-xs text-gray-400 uppercase tracking-wide mb-2">效果预览</p>
                    <div className="w-20 mx-auto">
                      {getCropModePreview(segment.crop_mode)}
                    </div>
                  </div>

                  <div className="mt-2 px-3 py-2 bg-gray-100 rounded-lg text-xs text-gray-500 flex items-start gap-2">
                    <span className="mt-0.5">💡</span>
                    <span>{getCropModeHint(segment.crop_mode)}</span>
                  </div>
                </div>

                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-2">片段时长 (秒)</label>
                  <input
                    type="number"
                    value={segment.duration}
                    onChange={(e) => handleSegmentChange(index, 'duration', parseInt(e.target.value) || 0)}
                    min="1"
                    className="w-full px-4 py-2.5 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent"
                  />
                </div>
              </div>
            )}
          </div>
        ))}
      </div>

      <div className={`p-4 rounded-lg flex items-center gap-3 ${
        isDurationValid ? 'bg-green-50 text-green-800' : 'bg-red-50 text-red-800'
      }`}>
        <span className="text-xl">{isDurationValid ? '✓' : '✗'}</span>
        <div>
          <p className="font-medium">
            时长验证: {formData.template_segments.map((s) => s.duration).join(' + ')} = {totalSegmentDuration} 秒
          </p>
          {!isDurationValid && (
            <p className="text-sm mt-1">
              {durationDiff > 0 ? `还差 ${durationDiff} 秒` : `超出 ${Math.abs(durationDiff)} 秒`}
            </p>
          )}
        </div>
      </div>
    </div>
  );

  const renderTutorialTab = () => (
    <div className="space-y-6">
      <div>
        <label className="block text-sm font-medium text-gray-700 mb-2">教程素材来源文件夹</label>
          <div className="flex gap-3">
            <button
              onClick={() => handleSelectFolder(true)}
              className="px-4 py-2.5 border border-gray-300 rounded-lg hover:bg-gray-50 transition-colors"
            >
              选择文件夹
            </button>
            <div className="flex-1 px-4 py-2.5 bg-gray-50 border border-gray-200 rounded-lg text-sm text-gray-600 flex items-center gap-2">
              <span>📁</span>
              <span className="truncate">
                {formData.tutorial_folder || '请选择文件夹...'}
              </span>
            </div>
          </div>
          <p className="text-xs text-gray-400 mt-2">
            说明：教程素材用于在模板片段之间插入过渡内容，相同片段不会重复使用
          </p>
      </div>
    </div>
  );

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="bg-white rounded-2xl shadow-2xl w-full max-w-4xl h-[640px] flex flex-col animate-fadeIn">
        <div className="bg-gradient-to-r from-gray-900 to-gray-800 px-6 py-4 flex justify-between items-center flex-shrink-0">
          <h3 className="text-white text-lg font-semibold flex items-center gap-2">
            <span>📝</span>
            <span>{config ? '编辑配置' : '新建配置'}</span>
          </h3>
          <button
            onClick={onClose}
            className="w-8 h-8 bg-gray-700 text-gray-400 rounded-lg flex items-center justify-center hover:bg-gray-600 hover:text-white transition-colors"
          >
            ×
          </button>
        </div>

        <div className="flex h-[600px]">
          <div className="w-48 bg-gray-50 border-r border-gray-200 py-4 flex-shrink-0">
            {tabs.map((tab) => (
              <button
                key={tab.key}
                onClick={() => setCurrentTab(tab.key)}
                className={`w-full px-4 py-3 text-left flex items-center gap-3 transition-colors ${
                  currentTab === tab.key
                    ? 'bg-white text-primary border-l-4 border-primary'
                    : 'text-gray-600 hover:bg-gray-100 border-l-4 border-transparent'
                }`}
              >
                <span className="text-lg">{tab.icon}</span>
                <span className="text-sm font-medium">{tab.label}</span>
              </button>
            ))}
          </div>

          <div className="flex-1 overflow-y-auto p-6">
            <h3 className="text-lg font-semibold text-gray-800 mb-6 flex items-center gap-2">
              <span>{tabs.find((t) => t.key === currentTab)?.icon}</span>
              <span>{tabs.find((t) => t.key === currentTab)?.label}</span>
            </h3>

            {currentTab === 'basic' && renderBasicTab()}
            {currentTab === 'template' && renderTemplateTab()}
            {currentTab === 'tutorial' && renderTutorialTab()}
          </div>
        </div>

        <div className="px-6 py-4 bg-gray-50 border-t border-gray-200 flex justify-between flex-shrink-0">
          <div className="flex gap-2">
            {currentTabIndex > 0 && (
              <button
                onClick={handlePrevTab}
                className="px-5 py-2.5 border border-gray-300 text-gray-700 rounded-lg hover:bg-gray-100 transition-colors flex items-center gap-2"
              >
                <span>←</span>
                <span>上一页</span>
              </button>
            )}
          </div>
          <div className="flex gap-3">
            <button
              onClick={onClose}
              disabled={isSubmitting}
              className="px-5 py-2.5 border border-gray-300 text-gray-700 rounded-lg hover:bg-gray-100 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              取消
            </button>
            {currentTabIndex < tabs.length - 1 ? (
              <button
                onClick={handleNextTab}
                className="px-5 py-2.5 bg-gradient-to-r from-primary to-primary-dark text-white rounded-lg hover:shadow-lg transition-all flex items-center gap-2"
              >
                <span>下一页</span>
                <span>→</span>
              </button>
            ) : (
              <button
                onClick={handleSubmit}
                disabled={isSubmitting || !isDurationValid}
                className="px-5 py-2.5 bg-gradient-to-r from-green-500 to-green-600 text-white rounded-lg hover:shadow-lg transition-all disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2"
              >
                {isSubmitting ? (
                  <>
                    <span>⏳</span>
                    <span>保存中...</span>
                  </>
                ) : (
                  <>
                    <span>✓</span>
                    <span>保存</span>
                  </>
                )}
              </button>
            )}
          </div>
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
