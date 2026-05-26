import { create } from 'zustand';
import type { AppConfig, DiscoveredDevice, SyncStats, TransferProgress } from '../types';
import * as commands from '../hooks/useTauriCommands';

interface AppState {
  // 在线设备列表
  devices: DiscoveredDevice[];
  // 应用配置
  config: AppConfig | null;
  // 同步状态
  syncEnabled: boolean;
  // 同步统计
  stats: SyncStats;
  // 传输进度列表
  transfers: TransferProgress[];

  // Actions
  loadConfig: () => Promise<void>;
  setSyncEnabled: (enabled: boolean) => Promise<void>;
  updateDeviceName: (name: string) => Promise<void>;
  updateConfig: (config: AppConfig) => Promise<void>;
  addDevice: (device: DiscoveredDevice) => void;
  removeDevice: (deviceId: string) => void;
  updateTransferProgress: (p: TransferProgress) => void;
  refreshStats: () => Promise<void>;
}

export const useAppStore = create<AppState>((set, get) => ({
  devices: [],
  config: null,
  syncEnabled: true,
  stats: { texts_synced: 0, images_synced: 0, files_transferred: 0 },
  transfers: [],

  loadConfig: async () => {
    try {
      const config = await commands.getConfig();
      const enabled = await commands.isSyncEnabled();
      set({ config, syncEnabled: enabled });
    } catch (e) {
      console.error('加载配置失败:', e);
    }
  },

  setSyncEnabled: async (enabled: boolean) => {
    try {
      // toggle_sync 切换当前状态，如果目标状态与当前不同才调用
      const current = get().syncEnabled;
      if (current !== enabled) {
        const result = await commands.toggleSync();
        set({ syncEnabled: result });
      }
    } catch (e) {
      console.error('切换同步失败:', e);
    }
  },

  updateDeviceName: async (name: string) => {
    try {
      await commands.updateDeviceName(name);
      const config = get().config;
      if (config) {
        set({ config: { ...config, device_name: name } });
      }
    } catch (e) {
      console.error('更新设备名失败:', e);
    }
  },

  updateConfig: async (config: AppConfig) => {
    try {
      await commands.updateConfig(config);
      set({ config });
    } catch (e) {
      console.error('更新配置失败:', e);
    }
  },

  addDevice: (device: DiscoveredDevice) => {
    set((state) => {
      const exists = state.devices.find((d) => d.device_id === device.device_id);
      if (exists) {
        return {
          devices: state.devices.map((d) =>
            d.device_id === device.device_id ? device : d
          ),
        };
      }
      return { devices: [...state.devices, device] };
    });
  },

  removeDevice: (deviceId: string) => {
    set((state) => ({
      devices: state.devices.filter((d) => d.device_id !== deviceId),
    }));
  },

  updateTransferProgress: (p: TransferProgress) => {
    set((state) => {
      const idx = state.transfers.findIndex((t) => t.file_name === p.file_name);
      if (idx >= 0) {
        const updated = [...state.transfers];
        updated[idx] = p;
        return { transfers: updated };
      }
      return { transfers: [...state.transfers, p] };
    });
  },

  refreshStats: async () => {
    try {
      const stats = await commands.getSyncStats();
      set({ stats });
    } catch (e) {
      console.error('获取同步统计失败:', e);
    }
  },
}));
