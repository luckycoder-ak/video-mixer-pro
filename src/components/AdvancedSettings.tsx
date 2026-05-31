import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { AppSettings } from '../types';

/**
 * 高级设置面板：当前承载飞书自定义机器人 Webhook URL 配置 + 测试发送按钮。
 *
 * 行为：
 * - 挂载时通过 `get_app_settings` 读取当前配置；
 * - 「保存」调用 `save_app_settings`，后端校验前缀（https://open.feishu.cn/）；
 * - 「测试」调用 `send_feishu_test_message`，错误信息原样回显。
 */
export function AdvancedSettings() {
  const [webhookUrl, setWebhookUrl] = useState('');
  const [originalUrl, setOriginalUrl] = useState('');
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [hint, setHint] = useState<{ kind: 'success' | 'error' | 'info'; text: string } | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const s = await invoke<AppSettings>('get_app_settings');
        setWebhookUrl(s.feishu_webhook_url || '');
        setOriginalUrl(s.feishu_webhook_url || '');
      } catch (e) {
        setHint({ kind: 'error', text: `读取设置失败: ${String(e)}` });
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  const showHint = (kind: 'success' | 'error' | 'info', text: string) => {
    setHint({ kind, text });
    setTimeout(() => setHint(null), 4000);
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      const trimmed = webhookUrl.trim();
      await invoke('save_app_settings', {
        settings: { feishu_webhook_url: trimmed } as AppSettings,
      });
      setOriginalUrl(trimmed);
      setWebhookUrl(trimmed);
      showHint('success', trimmed ? '已保存飞书 Webhook 配置' : '已清空飞书 Webhook 配置（通知将静默跳过）');
    } catch (e) {
      showHint('error', `保存失败: ${String(e)}`);
    } finally {
      setSaving(false);
    }
  };

  const handleTest = async () => {
    setTesting(true);
    try {
      await invoke('send_feishu_test_message');
      showHint('success', '测试消息发送成功，请检查飞书群聊');
    } catch (e) {
      showHint('error', `测试失败: ${String(e)}`);
    } finally {
      setTesting(false);
    }
  };

  const dirty = webhookUrl.trim() !== originalUrl.trim();
  const hasSavedWebhook = originalUrl.trim().length > 0;

  if (loading) {
    return <div className="p-8 text-center text-gray-500">加载中...</div>;
  }

  const hintColor =
    hint?.kind === 'success'
      ? 'bg-green-50 text-green-700 border-green-200'
      : hint?.kind === 'error'
      ? 'bg-red-50 text-red-700 border-red-200'
      : 'bg-blue-50 text-blue-700 border-blue-200';

  return (
    <div className="bg-white rounded-lg shadow-sm border border-gray-200 p-6 max-w-3xl">
      <h2 className="text-lg font-semibold text-gray-900 mb-1">飞书机器人通知</h2>
      <p className="text-sm text-gray-500 mb-6">
        配置自定义机器人 Webhook URL 后，合成任务（成功/失败/部分成功）与定时任务（成功/失败/IID 失效/进入冷却/自动停用）完成时将通过群聊推送通知。留空则静默跳过。
      </p>

      <div className="mb-4">
        <label className="block text-sm font-medium text-gray-700 mb-2">
          Webhook URL
        </label>
        <input
          type="text"
          value={webhookUrl}
          onChange={(e) => setWebhookUrl(e.target.value)}
          placeholder="https://open.feishu.cn/open-apis/bot/v2/hook/xxxxxxxx"
          className="w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-primary focus:border-transparent font-mono"
        />
        <p className="text-xs text-gray-500 mt-1">
          必须以 <code>https://open.feishu.cn/</code> 开头。
        </p>
      </div>

      <div className="flex items-center gap-3">
        <button
          onClick={handleSave}
          disabled={saving || !dirty}
          className="px-4 py-2 bg-primary text-white text-sm font-medium rounded-md hover:bg-primary-dark disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
        >
          {saving ? '保存中...' : '保存'}
        </button>
        <button
          onClick={handleTest}
          disabled={testing || !hasSavedWebhook || dirty}
          title={dirty ? '请先保存配置后再测试' : !hasSavedWebhook ? '请先保存 Webhook URL' : '发送测试消息到飞书群聊'}
          className="px-4 py-2 bg-white text-primary text-sm font-medium border border-primary rounded-md hover:bg-primary/5 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
        >
          {testing ? '发送中...' : '测试发送'}
        </button>
      </div>

      {hint && (
        <div className={`mt-4 px-4 py-2 rounded-md border text-sm ${hintColor}`}>
          {hint.text}
        </div>
      )}

      <div className="mt-8 pt-6 border-t border-gray-200">
        <h3 className="text-sm font-semibold text-gray-700 mb-2">如何获取 Webhook？</h3>
        <ol className="text-xs text-gray-500 space-y-1 list-decimal list-inside">
          <li>在飞书目标群聊设置中选择「机器人」→「添加机器人」→「自定义机器人」。</li>
          <li>填写名称后即可获得 Webhook URL，复制到上面的输入框。</li>
          <li>建议关闭「自定义关键词」与「IP 白名单」等限制，否则消息可能被拦截。</li>
        </ol>
      </div>
    </div>
  );
}
