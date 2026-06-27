// Package hooks installs Rigger's Claude Code integration. It merges a
// SessionStart hook into .claude/settings.json - preserving every other setting -
// so a Claude Code session opened in a Rigger repository starts primed with the
// project's recently recorded decisions.
package hooks

import (
	"encoding/json"
	"fmt"
	"strings"
)

// InstallSessionStart merges a SessionStart hook that runs command into the
// settings JSON. It is idempotent (installing twice does not duplicate the hook)
// and preserves all other settings. existing may be empty. It returns the new
// settings bytes.
func InstallSessionStart(existing []byte, command string) ([]byte, error) {
	root := map[string]any{}
	if len(strings.TrimSpace(string(existing))) > 0 {
		if err := json.Unmarshal(existing, &root); err != nil {
			return nil, fmt.Errorf("hooks: parse settings.json: %w", err)
		}
	}

	allHooks, _ := root["hooks"].(map[string]any)
	if allHooks == nil {
		allHooks = map[string]any{}
	}
	sessionStart, _ := allHooks["SessionStart"].([]any)

	if !hasCommand(sessionStart, command) {
		sessionStart = append(sessionStart, map[string]any{
			"matcher": "",
			"hooks": []any{
				map[string]any{"type": "command", "command": command},
			},
		})
	}
	allHooks["SessionStart"] = sessionStart
	root["hooks"] = allHooks

	out, err := json.MarshalIndent(root, "", "  ")
	if err != nil {
		return nil, fmt.Errorf("hooks: encode settings.json: %w", err)
	}
	return append(out, '\n'), nil
}

func hasCommand(sessionStart []any, command string) bool {
	for _, block := range sessionStart {
		m, ok := block.(map[string]any)
		if !ok {
			continue
		}
		inner, _ := m["hooks"].([]any)
		for _, h := range inner {
			if hm, ok := h.(map[string]any); ok && hm["command"] == command {
				return true
			}
		}
	}
	return false
}
