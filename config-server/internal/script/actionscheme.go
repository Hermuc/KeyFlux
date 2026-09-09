package script

import (
	"fmt"
	"regexp"
	"strings"

	"settings/internal/behaviors"
)

// 特征中文显示名 (保存校验的错误提示用, 与前端 i18n 保持一致)
var textTypeLabels = map[string]string{
	"url":    "链接",
	"path":   "路径",
	"magnet": "磁力链接",
	"plain":  "纯文本",
}

// matchTypeName 匹配类型中文显示名 (错误提示用)
func matchTypeName(matchType string) string {
	if matchType == "textType" {
		return "文本特征"
	}
	return "文件后缀"
}

// behaviorDisplayName 行为显示名: 优先包名, 缺失回退原始 ID
func behaviorDisplayName(cat *behaviors.Catalog, id string) string {
	if cat != nil {
		if p := cat.Get(id); p != nil {
			return p.Name
		}
	}
	return id
}

// ValidateFileGroups 校验文件分组表结构: 名称/显示名非空, 后缀列表非空 (分组为快捷填充数据, 结构非法时拒绝保存)
func ValidateFileGroups(groups []FileGroup) error {
	for i := range groups {
		g := &groups[i]
		if strings.TrimSpace(g.Name) == "" {
			return fmt.Errorf("文件分组第 %d 项缺少名称 (name)", i+1)
		}
		if strings.TrimSpace(g.Label) == "" {
			return fmt.Errorf("文件分组「%s」缺少显示名 (label)", g.Name)
		}
		if len(g.Exts) == 0 {
			return fmt.Errorf("文件分组「%s」的后缀列表 (exts) 为空", g.Name)
		}
	}
	return nil
}

// MatchSelectedAction 及其匹配语义已迁至 selectedaction.go (单键分发); 本文件保留
// matchActionRule/matchFileExt/matchTextType 供其复用。
func matchActionRule(rule *ActionRule, isFile bool, content string) bool {
	switch rule.MatchType {
	case "fileExt":
		if !isFile {
			return false
		}
		return matchFileExt(rule.MatchValue, content)
	case "textType":
		if isFile {
			return false
		}
		return matchTextType(rule.MatchValue, content)
	}
	return false
}

// matchFileExt 匹配文件后缀, MatchValue 支持逗号分隔多个后缀, "*" 匹配任意文件
func matchFileExt(matchValue, content string) bool {
	if matchValue == "*" {
		return true
	}
	exts := strings.Split(matchValue, ",")
	for _, line := range strings.Split(content, "\n") {
		// fileExt 返回含点后缀, 去掉点后与条件值比较 (与 AHK 端 SplitPath 返回不带点扩展名的语义一致)
		ext := strings.TrimPrefix(fileExt(line), ".")
		if ext == "" {
			continue
		}
		for _, v := range exts {
			v = strings.TrimSpace(v)
			v = strings.TrimPrefix(v, ".")
			if v == "*" || strings.EqualFold(v, ext) {
				return true
			}
		}
	}
	return false
}

// matchTextType 匹配文本特征: url(链接) / path(路径) / magnet(磁力链接) / plain(纯文本)
// 与 AHK 端 MatchTextType 保持一致
func matchTextType(t, content string) bool {
	reURL := regexp.MustCompile(`(?i)^(https?|ftp)://`)
	rePath := regexp.MustCompile(`^(\\\\[^\\]+\\[^\\]+|[a-zA-Z]:\\)`)
	reMagnet := regexp.MustCompile(`(?i)^magnet:`)
	switch strings.ToLower(strings.TrimSpace(t)) {
	case "url":
		return reURL.MatchString(content)
	case "path":
		return rePath.MatchString(content)
	case "magnet":
		return reMagnet.MatchString(content)
	case "plain":
		return !reURL.MatchString(content) && !rePath.MatchString(content) && !reMagnet.MatchString(content)
	}
	return false
}

// PreviewAction 生成执行预览: 把 %selected% 替换为选中内容, search 类型返回 URL 编码后的结果
func PreviewAction(rule *ActionRule, content string) string {
	switch rule.ActionType {
	case "search":
		return strings.ReplaceAll(rule.ActionValue, "%selected%", urlEncode(content))
	case "open_url":
		return "用默认浏览器打开: " + content
	case "open_path":
		return "按系统关联程序打开: " + content
	case "open_folder":
		return "打开选中路径所在文件夹"
	case "magnet_download":
		return "用默认 BT 下载工具下载: " + content
	default:
		return strings.ReplaceAll(rule.ActionValue, "%selected%", content)
	}
}

// urlEncode 与 AHK 端 URIEncode (Functions.ahk) 行为一致: 非保留字符原样输出, 其余按字节百分号编码
func urlEncode(s string) string {
	var buf strings.Builder
	for _, b := range []byte(s) {
		if (b >= '0' && b <= '9') || (b >= 'A' && b <= 'Z') || (b >= 'a' && b <= 'z') {
			buf.WriteByte(b)
		} else {
			buf.WriteString(fmt.Sprintf("%%%02X", b))
		}
	}
	return buf.String()
}

// fileExt 取文件后缀 (含点, 如 ".txt"), 无后缀返回空字符串
func fileExt(path string) string {
	path = strings.TrimSpace(path)
	idx := strings.LastIndexByte(path, '.')
	if idx < 0 || idx == len(path)-1 {
		return ""
	}
	return path[idx:]
}
