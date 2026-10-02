package server

// MigrateQuickSwitchSettings 的单测（2026-10-02 P5）。
//
// 三条规则各一例：
//  1. 旧段全零签名（旧配置缺段）⇒ 不迁移（插件回落 manifest 默认）；
//  2. 非全零 ⇒ 9 键全量入表（bool → "true"/"false"，int → 十进制，数组 → 换行分隔）；
//  3. 幂等 + 不覆盖：存储里已有的键原样保留（用户已存值不被迁移冲掉），旧段原样保留。

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

)

func writeMigrateConfig(t *testing.T, quickSwitch any) string {
	t.Helper()
	doc := map[string]any{
		"keymaps": []any{},
		"options": map[string]any{"quickSwitch": quickSwitch},
	}
	raw, err := json.Marshal(doc)
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "config.json")
	if err := os.WriteFile(path, raw, 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestMigrateQuickSwitchSettings(t *testing.T) {
	t.Run("全零签名不迁移", func(t *testing.T) {
		dir := withTempPluginDirs(t)
		path := writeMigrateConfig(t, map[string]any{
			"collectEnabled": false, "autoShow": false, "autoJumpOpen": false,
			"autoJumpSave": false, "pollIntervalMs": 0, "maxHistory": 0,
			"overlayRows": 0, "overlayRowsCompact": 0, "excludedPrefixes": nil,
		})
		MigrateQuickSwitchSettings(path)
		if got := pluginSettings.LoadFor("quick_switch"); len(got) != 0 {
			t.Fatalf("全零段不应迁移, 得到: %v", got)
		}
		_ = dir
	})

	t.Run("非全零全量迁移", func(t *testing.T) {
		withTempPluginDirs(t)
		path := writeMigrateConfig(t, map[string]any{
			"collectEnabled": true, "autoShow": true, "autoJumpOpen": false,
			"autoJumpSave": false, "pollIntervalMs": 800, "maxHistory": 200,
			"overlayRows": 8, "overlayRowsCompact": 4,
			"excludedPrefixes": []string{"D:\\Archive", "C:\\Temp"},
		})
		MigrateQuickSwitchSettings(path)
		got := pluginSettings.LoadFor("quick_switch")
		want := map[string]string{
			"collectEnabled": "true", "autoShow": "true", "autoJumpOpen": "false",
			"autoJumpSave": "false", "pollIntervalMs": "800", "maxHistory": "200",
			"overlayRows": "8", "overlayRowsCompact": "4",
			"excludedPrefixes": "D:\\Archive\nC:\\Temp",
		}
		for k, v := range want {
			if got[k] != v {
				t.Errorf("键 %q = %q, want %q", k, got[k], v)
			}
		}
		if len(got) != len(want) {
			t.Errorf("迁移键数 %d != %d: %v", len(got), len(want), got)
		}
	})

	t.Run("幂等且不覆盖已有键", func(t *testing.T) {
		withTempPluginDirs(t)
		// 用户已通过设置界面存过 pollIntervalMs = 500
		if err := pluginSettings.Save("quick_switch", map[string]string{"pollIntervalMs": "500"}); err != nil {
			t.Fatal(err)
		}
		path := writeMigrateConfig(t, map[string]any{
			"collectEnabled": true, "autoShow": true, "autoJumpOpen": true,
			"autoJumpSave": false, "pollIntervalMs": 800, "maxHistory": 0,
			"overlayRows": 0, "overlayRowsCompact": 0, "excludedPrefixes": nil,
		})
		MigrateQuickSwitchSettings(path)
		got := pluginSettings.LoadFor("quick_switch")
		if got["pollIntervalMs"] != "500" {
			t.Errorf("已有键被迁移覆盖: pollIntervalMs = %q, want 500", got["pollIntervalMs"])
		}
		if got["collectEnabled"] != "true" {
			t.Errorf("缺失键未迁移: collectEnabled = %q", got["collectEnabled"])
		}
		// 再跑一遍: 全部键已在, 无新增 (幂等)
		before := pluginSettings.LoadFor("quick_switch")
		MigrateQuickSwitchSettings(path)
		after := pluginSettings.LoadFor("quick_switch")
		if len(before) != len(after) {
			t.Errorf("重复迁移改变了键数: %d -> %d", len(before), len(after))
		}
	})
}
