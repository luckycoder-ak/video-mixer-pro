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
  const [expandedSegments, setExpandedSegments] = useState<Set<number>>(new Set());
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [currentTab, setCurrentTab] = useState<TabType>('basic');
  const [subtitleStyleExpanded, setSubtitleStyleExpanded] = useState(false);
  const [gradientExpanded, setGradientExpanded] = useState(false);

  const tabs: { key: TabType; label: string; icon: string }[] = [
    { key: 'basic', label: '基础信息配置', icon: '⚙️' },
    { key: 'template', label: '模板片段配置', icon: '🎬' },
    { key: 'tutorial', label: '教程素材配置', icon: '📚' },
  ];

  const currentTabIndex = tabs.findIndex((t) => t.key === currentTab);

  useEffect(() => {
    if (config) {
      setFormData(config);
      setExpandedSegments(new Set());
    } else {
      setFormData(createDefaultConfig());
      setExpandedSegments(new Set());
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

    const total = formData.template_duration;
    const avgDuration = total > 0 ? Math.round((total / count) * 10) / 10 : 0;
    const segments: TemplateSegment[] = [];
    let allocated = 0;

    for (let i = 1; i <= count; i++) {
      const existing = formData.template_segments.find((s) => s.segment_index === i);
      const base = existing
        ? { ...existing }
        : {
            segment_index: i,
            source_folder: '',
            source_folder2: '',
            crop_mode: 'single' as const,
            duration: avgDuration,
            scale_percent: 51,
          };

      if (!existing) {
        if (i === count) {
          const remaining = Math.round((total - allocated) * 10) / 10;
          base.duration = remaining > 0 ? remaining : avgDuration;
        } else {
          base.duration = avgDuration;
        }
      }

      allocated += base.duration;
      segments.push(base);
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

  const handleSubtitleStyleChange = (field: string, value: any) => {
    setFormData((prev) => ({
      ...prev,
      subtitle_style: { ...prev.subtitle_style, [field]: value },
    }));
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

  const handleSelectSubtitle = async () => {
    if (!isTauriEnv) {
      console.warn('请在 Tauri 应用中运行此功能');
      return;
    }
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'Subtitle', extensions: ['srt'] }],
      });
      if (selected) {
        handleInputChange('subtitle_path', selected);
      }
    } catch (error) {
      console.error('选择字幕文件失败:', error);
    }
  };

  const handleSelectFolder = async (isTutorialFolder: boolean, index?: number, isRootFolder?: boolean, isSecondFolder?: boolean) => {
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
          if (isSecondFolder) {
            handleSegmentChange(index, 'source_folder2', selected);
          } else {
            handleSegmentChange(index, 'source_folder', selected);
          }
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

  const getCropModeHint = (mode: string) => {
    switch (mode) {
      case 'single':
        return '单视频模式：直接截取指定时长，保持原始宽高比';
      case 'dual':
        return '双列模式：两个视频并排显示在中间区域，上下部分显示模糊背景';
      case 'quadrant':
        return '四宫格模式：四个视频等比例缩放至1/4宽度，田字格排列';
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
            <option value="4:5">4:5 (竖版社交)</option>
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
        <label className="block text-sm font-medium text-gray-700 mb-2">
          字幕文件 <span className="text-gray-400">(可选，仅支持 .srt 格式)</span>
        </label>
        <div className="flex gap-3">
          <button
            onClick={handleSelectSubtitle}
            className="px-4 py-2.5 border border-gray-300 rounded-lg hover:bg-gray-50 transition-colors"
          >
            选择字幕文件
          </button>
          <div className="flex-1 px-4 py-2.5 bg-gray-50 border border-gray-200 rounded-lg text-sm text-gray-600 flex items-center gap-2">
            <span>📝</span>
            <span className="truncate">
              {formData.subtitle_path || '未选择（可选）'}
            </span>
          </div>
          {formData.subtitle_path && (
            <button
              onClick={() => handleInputChange('subtitle_path', '')}
              className="px-4 py-2.5 border border-red-300 text-red-600 rounded-lg hover:bg-red-50 transition-colors"
            >
              清除
            </button>
          )}
        </div>
      </div>

      {/* 字幕样式实时预览 */}
      {formData.subtitle_path && (
        <div className="border border-gray-200 rounded-xl overflow-hidden bg-gray-900">
          <div className="px-4 py-2 bg-gray-800 border-b border-gray-700 flex justify-between items-center">
            <h3 className="font-semibold text-gray-300 text-sm">👁️ 字幕预览</h3>
            <span className="text-xs text-gray-500">实时预览（近似 FFmpeg 渲染效果）</span>
          </div>
          <div className="flex justify-center py-4">
            <div className="relative w-[180px] h-[320px] bg-gradient-to-b from-gray-800 to-gray-900 rounded-lg overflow-hidden shadow-2xl">
              {/* 模拟视频画面 */}
              <div className="absolute inset-0 flex items-center justify-center">
                <div className="text-center opacity-30">
                  <div className="text-3xl mb-2">🎬</div>
                  <div className="text-xs text-gray-400">视频画面</div>
                </div>
              </div>
              {/* 模拟字幕 */}
              <div
                className="absolute left-1/2 px-1"
                style={{
                  bottom: (() => {
                    const y = formData.subtitle_style.y;
                    if (y === 'h-th-100' || y === 'h-th-100') return '80px';
                    if (y === 'h-th-50') return '40px';
                    if (y === 'h-th-20') return '20px';
                    if (y === 'h-th-150') return '130px';
                    if (y === 'h/2') return '50%';
                    if (y === '100') return undefined;
                    if (y === 'w-tw-10') return '80px';
                    if (y === '(w-tw)/4') return '80px';
                    if (y === '3*(w-tw)/4') return '80px';
                    return '80px';
                  })(),
                  top: (() => {
                    const y = formData.subtitle_style.y;
                    if (y === '100') return '90px';
                    return undefined;
                  })(),
                  transform: (() => {
                    const x = formData.subtitle_style.x;
                    if (x === '(w-tw)/2') return 'translateX(-50%)';
                    if (x === 'w-tw-10') return 'none';
                    return 'translateX(-50%)';
                  })(),
                  textAlign: (() => {
                    const x = formData.subtitle_style.x;
                    if (x === '10') return 'left';
                    if (x === 'w-tw-10') return 'right';
                    return 'center';
                  })(),
                  width: (() => {
                    const x = formData.subtitle_style.x;
                    if (x === '10') return 'calc(100% - 20px)';
                    if (x === 'w-tw-10') return 'calc(100% - 20px)';
                    return '90%';
                  })(),
                  left: (() => {
                    const x = formData.subtitle_style.x;
                    if (x === '10') return '10px';
                    if (x === 'w-tw-10') return '10px';
                    return '50%';
                  })(),
                  fontSize: `${Math.max(10, formData.subtitle_style.fontsize * 0.5)}px`,
                  color: (() => {
                    const c = formData.subtitle_style.fontcolor;
                    if (c.startsWith('0x') || c.startsWith('0X')) return `#${c.slice(2)}`;
                    const namedColors: Record<string, string> = { white: '#FFFFFF', black: '#000000', yellow: '#FFFF00', red: '#FF0000', green: '#00FF00', blue: '#0000FF' };
                    return namedColors[c.toLowerCase()] || '#FFFFFF';
                  })(),
                  textShadow: [
                    formData.subtitle_style.borderw > 0 && (() => {
                      const bc = (() => {
                        const c = formData.subtitle_style.bordercolor;
                        if (c.startsWith('0x') || c.startsWith('0X')) return `#${c.slice(2)}`;
                        const namedColors: Record<string, string> = { black: '#000000', white: '#FFFFFF' };
                        return namedColors[c.toLowerCase()] || '#000000';
                      })();
                      return `-${formData.subtitle_style.borderw}px -${formData.subtitle_style.borderw}px 0 ${bc}, ${formData.subtitle_style.borderw}px -${formData.subtitle_style.borderw}px 0 ${bc}, -${formData.subtitle_style.borderw}px ${formData.subtitle_style.borderw}px 0 ${bc}, ${formData.subtitle_style.borderw}px ${formData.subtitle_style.borderw}px 0 ${bc}`;
                    })(),
                    (formData.subtitle_style.shadowx !== 0 || formData.subtitle_style.shadowy !== 0) && (() => {
                      const sc = (() => {
                        const c = formData.subtitle_style.shadowcolor;
                        if (c.startsWith('0x') || c.startsWith('0X')) return `#${c.slice(2)}`;
                        const namedColors: Record<string, string> = { white: '#FFFFFF', black: '#000000' };
                        return namedColors[c.toLowerCase()] || '#FFFFFF';
                      })();
                      return `${formData.subtitle_style.shadowx}px ${formData.subtitle_style.shadowy}px 4px ${sc}`;
                    })(),
                  ].filter(Boolean).join(', ') || 'none',
                  lineHeight: 1.4 + (formData.subtitle_style.line_spacing || 8) / 100,
                  fontWeight: 'bold',
                }}
              >
                {formData.subtitle_style.enable_gradient ? (
                  <span
                    style={{
                      background: `linear-gradient(to right, ${formData.subtitle_style.gradient_color1.startsWith('0x') ? `#${formData.subtitle_style.gradient_color1.slice(2)}` : '#FF6B6B'}, ${formData.subtitle_style.gradient_color2.startsWith('0x') ? `#${formData.subtitle_style.gradient_color2.slice(2)}` : '#4ECDC4'})`,
                      WebkitBackgroundClip: 'text',
                      WebkitTextFillColor: 'transparent',
                      backgroundClip: 'text',
                      filter: formData.subtitle_style.borderw > 0 ? (() => {
                        const bc = (() => {
                          const c = formData.subtitle_style.bordercolor;
                          if (c.startsWith('0x') || c.startsWith('0X')) return `#${c.slice(2)}`;
                          return '#000000';
                        })();
                        return `drop-shadow(0 0 ${formData.subtitle_style.borderw}px ${bc})`;
                      })() : 'none',
                    }}
                  >
                    这是一段字幕预览文字
                  </span>
                ) : (
                  '这是一段字幕预览文字'
                )}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* 字幕样式配置 */}
      {formData.subtitle_path && (
        <div className="border border-gray-200 rounded-xl overflow-hidden">
          <div
            className="px-4 py-3 bg-gray-50 border-b border-gray-200 flex justify-between items-center cursor-pointer hover:bg-gray-100 transition-colors"
            onClick={() => setSubtitleStyleExpanded(!subtitleStyleExpanded)}
          >
            <h3 className="font-semibold text-gray-800 text-sm">🎨 字幕样式配置</h3>
            <span className={`text-gray-500 text-xs transition-transform ${subtitleStyleExpanded ? 'rotate-180' : ''}`}>▼</span>
          </div>

          {subtitleStyleExpanded && (
            <div className="p-4 space-y-4">
              {/* 基础样式 */}
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-xs font-medium text-gray-600 mb-1.5">字号</label>
                  <input
                    type="number"
                    value={formData.subtitle_style.fontsize}
                    onChange={(e) => handleSubtitleStyleChange('fontsize', parseInt(e.target.value) || 36)}
                    min="12"
                    max="120"
                    className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent text-sm"
                  />
                </div>
                <div>
                  <label className="block text-xs font-medium text-gray-600 mb-1.5">描边宽度</label>
                  <input
                    type="number"
                    value={formData.subtitle_style.borderw}
                    onChange={(e) => handleSubtitleStyleChange('borderw', parseInt(e.target.value) || 0)}
                    min="0"
                    max="10"
                    className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent text-sm"
                  />
                </div>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-xs font-medium text-gray-600 mb-1.5">字体颜色</label>
                  <div className="flex gap-2">
                    <input
                      type="color"
                      value={formData.subtitle_style.fontcolor.startsWith('0x') ? `#${formData.subtitle_style.fontcolor.slice(2)}` : formData.subtitle_style.fontcolor === 'white' ? '#FFFFFF' : formData.subtitle_style.fontcolor === 'black' ? '#000000' : formData.subtitle_style.fontcolor === 'yellow' ? '#FFFF00' : '#FFFFFF'}
                      onChange={(e) => {
                        const hex = e.target.value.replace('#', '').toUpperCase();
                        handleSubtitleStyleChange('fontcolor', `0x${hex}`);
                      }}
                      className="w-10 h-10 rounded border border-gray-300 cursor-pointer"
                    />
                    <input
                      type="text"
                      value={formData.subtitle_style.fontcolor}
                      onChange={(e) => handleSubtitleStyleChange('fontcolor', e.target.value)}
                      placeholder="white 或 0xRRGGBB"
                      className="flex-1 px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent text-sm font-mono"
                    />
                  </div>
                </div>
                <div>
                  <label className="block text-xs font-medium text-gray-600 mb-1.5">描边颜色</label>
                  <div className="flex gap-2">
                    <input
                      type="color"
                      value={formData.subtitle_style.bordercolor.startsWith('0x') ? `#${formData.subtitle_style.bordercolor.slice(2)}` : formData.subtitle_style.bordercolor === 'white' ? '#FFFFFF' : formData.subtitle_style.bordercolor === 'black' ? '#000000' : '#000000'}
                      onChange={(e) => {
                        const hex = e.target.value.replace('#', '').toUpperCase();
                        handleSubtitleStyleChange('bordercolor', `0x${hex}`);
                      }}
                      className="w-10 h-10 rounded border border-gray-300 cursor-pointer"
                    />
                    <input
                      type="text"
                      value={formData.subtitle_style.bordercolor}
                      onChange={(e) => handleSubtitleStyleChange('bordercolor', e.target.value)}
                      placeholder="black 或 0xRRGGBB"
                      className="flex-1 px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent text-sm font-mono"
                    />
                  </div>
                </div>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-xs font-medium text-gray-600 mb-1.5">阴影/发光色</label>
                  <div className="flex gap-2">
                    <input
                      type="color"
                      value={formData.subtitle_style.shadowcolor.startsWith('0x') ? `#${formData.subtitle_style.shadowcolor.slice(2)}` : formData.subtitle_style.shadowcolor === 'white' ? '#FFFFFF' : formData.subtitle_style.shadowcolor === 'black' ? '#000000' : '#FFFFFF'}
                      onChange={(e) => {
                        const hex = e.target.value.replace('#', '').toUpperCase();
                        handleSubtitleStyleChange('shadowcolor', `0x${hex}`);
                      }}
                      className="w-10 h-10 rounded border border-gray-300 cursor-pointer"
                    />
                    <input
                      type="text"
                      value={formData.subtitle_style.shadowcolor}
                      onChange={(e) => handleSubtitleStyleChange('shadowcolor', e.target.value)}
                      placeholder="white 或 0xRRGGBB"
                      className="flex-1 px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent text-sm font-mono"
                    />
                  </div>
                </div>
                <div>
                  <label className="block text-xs font-medium text-gray-600 mb-1.5">阴影偏移 (X, Y)</label>
                  <div className="flex gap-2">
                    <input
                      type="number"
                      value={formData.subtitle_style.shadowx}
                      onChange={(e) => handleSubtitleStyleChange('shadowx', parseInt(e.target.value) || 0)}
                      className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent text-sm"
                    />
                    <input
                      type="number"
                      value={formData.subtitle_style.shadowy}
                      onChange={(e) => handleSubtitleStyleChange('shadowy', parseInt(e.target.value) || 0)}
                      className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent text-sm"
                    />
                  </div>
                </div>
              </div>

              {/* 位置配置 */}
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-xs font-medium text-gray-600 mb-1.5">水平位置</label>
                  <select
                    value={formData.subtitle_style.x}
                    onChange={(e) => handleSubtitleStyleChange('x', e.target.value)}
                    className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent text-sm"
                  >
                    <option value="(w-tw)/2">水平居中</option>
                    <option value="10">左对齐 (偏移10px)</option>
                    <option value="w-tw-10">右对齐 (偏移10px)</option>
                    <option value="(w-tw)/4">左1/4处</option>
                    <option value="3*(w-tw)/4">右3/4处</option>
                  </select>
                </div>
                <div>
                  <label className="block text-xs font-medium text-gray-600 mb-1.5">垂直位置</label>
                  <select
                    value={formData.subtitle_style.y}
                    onChange={(e) => handleSubtitleStyleChange('y', e.target.value)}
                    className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent text-sm"
                  >
                    <option value="h-th-100">底部 (距底100px)</option>
                    <option value="h-th-50">底部 (距底50px)</option>
                    <option value="h-th-20">底部 (距底20px)</option>
                    <option value="h/2">屏幕中央</option>
                    <option value="100">顶部 (距顶100px)</option>
                    <option value="h-th-150">底部 (距底150px)</option>
                  </select>
                </div>
              </div>

              <div>
                <label className="block text-xs font-medium text-gray-600 mb-1.5">行间距</label>
                <input
                  type="number"
                  value={formData.subtitle_style.line_spacing}
                  onChange={(e) => handleSubtitleStyleChange('line_spacing', parseInt(e.target.value) || 0)}
                  min="0"
                  max="50"
                  className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent text-sm"
                />
              </div>

              {/* 逐字渐变配置 */}
              <div className="border border-gray-200 rounded-lg overflow-hidden">
                <div
                  className="px-3 py-2 bg-gray-50 border-b border-gray-200 flex justify-between items-center cursor-pointer hover:bg-gray-100 transition-colors"
                  onClick={() => setGradientExpanded(!gradientExpanded)}
                >
                  <label className="flex items-center gap-2 cursor-pointer text-sm">
                    <input
                      type="checkbox"
                      checked={formData.subtitle_style.enable_gradient}
                      onChange={(e) => handleSubtitleStyleChange('enable_gradient', e.target.checked)}
                      className="w-4 h-4 text-primary border-gray-300 rounded focus:ring-primary"
                    />
                    <span className="font-medium text-gray-700">启用逐字渐变</span>
                  </label>
                  <span className={`text-gray-500 text-xs transition-transform ${gradientExpanded ? 'rotate-180' : ''}`}>▼</span>
                </div>

                {gradientExpanded && formData.subtitle_style.enable_gradient && (
                  <div className="p-3 space-y-3">
                    <div className="grid grid-cols-2 gap-4">
                      <div>
                        <label className="block text-xs font-medium text-gray-600 mb-1.5">渐变起始色</label>
                        <div className="flex gap-2">
                          <input
                            type="color"
                            value={formData.subtitle_style.gradient_color1.startsWith('0x') ? `#${formData.subtitle_style.gradient_color1.slice(2)}` : '#FF6B6B'}
                            onChange={(e) => {
                              const hex = e.target.value.replace('#', '').toUpperCase();
                              handleSubtitleStyleChange('gradient_color1', `0x${hex}`);
                            }}
                            className="w-10 h-10 rounded border border-gray-300 cursor-pointer"
                          />
                          <input
                            type="text"
                            value={formData.subtitle_style.gradient_color1}
                            onChange={(e) => handleSubtitleStyleChange('gradient_color1', e.target.value)}
                            placeholder="0xFF6B6B"
                            className="flex-1 px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent text-sm font-mono"
                          />
                        </div>
                      </div>
                      <div>
                        <label className="block text-xs font-medium text-gray-600 mb-1.5">渐变结束色</label>
                        <div className="flex gap-2">
                          <input
                            type="color"
                            value={formData.subtitle_style.gradient_color2.startsWith('0x') ? `#${formData.subtitle_style.gradient_color2.slice(2)}` : '#4ECDC4'}
                            onChange={(e) => {
                              const hex = e.target.value.replace('#', '').toUpperCase();
                              handleSubtitleStyleChange('gradient_color2', `0x${hex}`);
                            }}
                            className="w-10 h-10 rounded border border-gray-300 cursor-pointer"
                          />
                          <input
                            type="text"
                            value={formData.subtitle_style.gradient_color2}
                            onChange={(e) => handleSubtitleStyleChange('gradient_color2', e.target.value)}
                            placeholder="0x4ECDC4"
                            className="flex-1 px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent text-sm font-mono"
                          />
                        </div>
                      </div>
                    </div>

                    {/* 渐变预览 */}
                    <div className="p-3 bg-gray-100 rounded-lg">
                      <div
                        className="text-center font-bold text-lg"
                        style={{
                          background: `linear-gradient(to right, ${formData.subtitle_style.gradient_color1.startsWith('0x') ? `#${formData.subtitle_style.gradient_color1.slice(2)}` : '#FF6B6B'}, ${formData.subtitle_style.gradient_color2.startsWith('0x') ? `#${formData.subtitle_style.gradient_color2.slice(2)}` : '#4ECDC4'})`,
                          WebkitBackgroundClip: 'text',
                          WebkitTextFillColor: 'transparent',
                          backgroundClip: 'text',
                        }}
                      >
                        字幕渐变效果预览
                      </div>
                    </div>
                  </div>
                )}
              </div>

              {/* 重置按钮 */}
              <button
                onClick={() => {
                  handleSubtitleStyleChange('fontsize', 36);
                  handleSubtitleStyleChange('fontcolor', 'white');
                  handleSubtitleStyleChange('borderw', 3);
                  handleSubtitleStyleChange('bordercolor', 'black');
                  handleSubtitleStyleChange('shadowcolor', 'white');
                  handleSubtitleStyleChange('shadowx', 2);
                  handleSubtitleStyleChange('shadowy', 2);
                  handleSubtitleStyleChange('x', '(w-tw)/2');
                  handleSubtitleStyleChange('y', 'h-th-100');
                  handleSubtitleStyleChange('line_spacing', 8);
                  handleSubtitleStyleChange('enable_gradient', false);
                  handleSubtitleStyleChange('gradient_color1', '0xFF6B6B');
                  handleSubtitleStyleChange('gradient_color2', '0x4ECDC4');
                }}
                className="text-xs text-gray-500 hover:text-gray-700 transition-colors"
              >
                ↺ 重置为默认值
              </button>
            </div>
          )}
        </div>
      )}

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

      {/* 高级参数配置 */}
      <div className="border border-gray-200 rounded-xl overflow-hidden">
        <div className="px-4 py-3 bg-gray-50 border-b border-gray-200">
          <h3 className="font-semibold text-gray-800">⚙️ 高级参数配置</h3>
        </div>
        <div className="p-4 space-y-3">
          <label className="flex items-center gap-3 cursor-pointer">
            <input
              type="checkbox"
              checked={formData.enable_transition}
              onChange={(e) => handleInputChange('enable_transition', e.target.checked)}
              className="w-5 h-5 text-primary border-gray-300 rounded focus:ring-primary"
            />
            <span className="text-sm font-medium text-gray-700">是否启用转场效果</span>
            <span className="text-xs text-gray-400">（默认不启用）</span>
          </label>
        </div>
      </div>
    </div>
  );

  const renderTemplateTab = () => (
    <div className="space-y-6">
      <div className="grid grid-cols-1 gap-4 mb-4">
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
                {segment.source_folder && (
                  <span className="px-2 py-0.5 bg-green-100 text-green-700 text-xs rounded">已设置</span>
                )}
              </div>
              <div className="flex items-center gap-4">
                <div className="flex items-center gap-2">
                  <span className="px-2 py-1 bg-blue-100 text-blue-700 text-xs rounded">
                    {segment.crop_mode === 'single' ? '单视频' : segment.crop_mode === 'dual' ? '双列' : '四宫格'}
                  </span>
                  <span className="text-gray-500 text-sm">{segment.duration}秒</span>
                </div>
                <span className={`text-gray-500 transition-transform ${expandedSegments.has(segment.segment_index) ? 'rotate-180' : ''}`}>
                  ▼
                </span>
              </div>
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

                {segment.crop_mode === 'dual' && (
                  <div>
                    <label className="block text-sm font-medium text-gray-700 mb-2">
                      双列第二素材文件夹 <span className="text-gray-400">(可选)</span>
                    </label>
                    <div className="flex gap-3">
                      <button
                        onClick={() => handleSelectFolder(false, index, false, true)}
                        className="px-4 py-2 border border-gray-300 rounded-lg hover:bg-gray-50 transition-colors text-sm"
                      >
                        选择文件夹
                      </button>
                      <div className="flex-1 px-4 py-2 bg-white border border-gray-200 rounded-lg text-sm text-gray-600 flex items-center gap-2">
                        <span>📁</span>
                        <span className="truncate">
                          {segment.source_folder2 || '未选择（将从第一个文件夹随机抽取）'}
                        </span>
                      </div>
                      {segment.source_folder2 && (
                        <button
                          onClick={() => handleSegmentChange(index, 'source_folder2', '')}
                          className="px-4 py-2 border border-red-300 text-red-600 rounded-lg hover:bg-red-50 transition-colors text-sm"
                        >
                          清除
                        </button>
                      )}
                    </div>
                    <p className="text-xs text-gray-400 mt-1">如未选择，左右两个视频均从第一个文件夹随机抽取</p>
                  </div>
                )}

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

                  <div className="px-3 py-2 bg-gray-100 rounded-lg text-xs text-gray-500 flex items-start gap-2">
                    <span className="mt-0.5">💡</span>
                    <span>{getCropModeHint(segment.crop_mode)}</span>
                  </div>
                </div>

                {segment.crop_mode === 'dual' && (
                  <div>
                    <label className="block text-sm font-medium text-gray-700 mb-2">
                      双列缩放比例 (%) <span className="text-gray-400">(≥50)</span>
                    </label>
                    <input
                      type="number"
                      value={segment.scale_percent}
                      onChange={(e) => handleSegmentChange(index, 'scale_percent', parseInt(e.target.value) || 51)}
                      onBlur={(e) => {
                        const val = parseInt(e.target.value) || 51;
                        if (val < 50) {
                          handleSegmentChange(index, 'scale_percent', 50);
                        }
                      }}
                      min="50"
                      step="1"
                      className="w-full px-4 py-2.5 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent"
                    />
                    <p className="text-xs text-gray-400 mt-1">视频按此比例缩放后裁剪到半屏区域，值越大画面显示越多</p>
                  </div>
                )}

                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-2">片段时间点 (秒)</label>
                  <input
                    type="number"
                    value={segment.duration}
                    onChange={(e) => handleSegmentChange(index, 'duration', parseFloat(e.target.value) || 0)}
                    min="0.1"
                    step="0.01"
                    className="w-full px-4 py-2.5 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary focus:border-transparent"
                  />
                  <p className="text-xs text-gray-400 mt-1">该片段的结束时间点，前一片段的终点即为当前片段的起点</p>
                </div>
              </div>
            )}
          </div>
        ))}
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
      </div>
    </div>
  );

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="bg-white rounded-2xl shadow-2xl w-full max-w-5xl h-[700px] flex flex-col animate-fadeIn">
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

        <div className="flex flex-1 overflow-hidden">
          <div className="w-52 bg-gray-50 border-r border-gray-200 py-4 flex-shrink-0">
            {tabs.map((tab) => (
              <button
                key={tab.key}
                onClick={() => setCurrentTab(tab.key)}
                className={`w-full px-5 py-3.5 text-left flex items-center gap-3 transition-colors ${
                  currentTab === tab.key
                    ? 'bg-white text-primary border-l-4 border-primary'
                    : 'text-gray-600 hover:bg-gray-100 border-l-4 border-transparent'
                }`}
              >
                <span className="text-xl">{tab.icon}</span>
                <span className="text-sm font-medium">{tab.label}</span>
              </button>
            ))}
          </div>

          <div className="flex-1 overflow-y-auto p-5">
            <h3 className="text-lg font-semibold text-gray-800 mb-4 flex items-center gap-2">
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
                disabled={isSubmitting}
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
