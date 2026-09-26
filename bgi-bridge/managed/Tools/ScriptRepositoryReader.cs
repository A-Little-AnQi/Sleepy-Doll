using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using BgiBridge.Protocol;

namespace BgiBridge.Tools;

/// <summary>中央仓库索引搜索与文本分页，不检出或订阅文件。</summary>
public static class ScriptRepositoryReader
{
    public static string NormalizePath(string path)
    {
        var normalized = path.Trim().Replace('\\', '/');
        var parts = normalized.Split('/');
        if (normalized.Length == 0 || normalized.Length > 1024 || normalized.Contains(':')
            || parts.Any(part => part is "" or "." or ".." || part.Any(char.IsControl) || part.EndsWith(' ') || part.EndsWith('.'))
            || !new[] { "js", "pathing", "combat", "tcg" }.Contains(parts[0], StringComparer.Ordinal))
            throw BridgeException.InvalidArgument("path 必须是仓库内容的相对路径，如 js/脚本目录/settings.json；不接受绝对路径或 ..。");
        return normalized;
    }

    public static JsonElement? FindNode(string index, string path)
    {
        path = NormalizePath(path);
        using var document = JsonDocument.Parse(index);
        var nodes = document.RootElement.GetProperty("indexes");
        JsonElement? found = null;
        foreach (var part in path.Split('/'))
        {
            if (nodes.ValueKind != JsonValueKind.Array) return null;
            found = nodes.EnumerateArray().FirstOrDefault(node => node.GetProperty("name").GetString() == part);
            if (found.Value.ValueKind == JsonValueKind.Undefined) return null;
            nodes = found.Value.TryGetProperty("children", out var children) ? children : default;
        }
        return found?.Clone();
    }

    private static string Description(JsonElement node) => node.TryGetProperty("description", out var d)
        ? (d.GetString() ?? "")[..Math.Min(d.GetString()?.Length ?? 0, 1200)] : "";

    private static string[] Requirements(JsonElement node)
    {
        var requirements = new HashSet<string>(StringComparer.Ordinal);
        void Visit(JsonElement item)
        {
            var description = Description(item);
            if (description.Contains("必须") || description.Contains("需要角色")) requirements.Add(description);
            if (item.TryGetProperty("children", out var children))
                foreach (var child in children.EnumerateArray()) Visit(child);
        }
        Visit(node);
        return requirements.Take(8).ToArray();
    }

    public static object Search(string index, string query, string category, int offset, int limit)
    {
        using var document = JsonDocument.Parse(index);
        var matches = new List<(int Score, string Path, JsonElement Node, bool Named)>();
        var needle = query.Trim();
        if (needle.Length == 0 || needle.Length > 200)
            throw BridgeException.InvalidArgument("query 必须是 1 到 200 字符的标题或功能词。");
        void Visit(JsonElement node, string parent, bool covered)
        {
            var name = node.GetProperty("name").GetString()!;
            if (parent.Length == 0 && category != "all" && name != category) return;
            var path = parent.Length == 0 ? name : $"{parent}/{name}";
            var description = Description(node);
            var title = description.Split("~|~", 2)[0];
            var exact = name.Equals(needle, StringComparison.OrdinalIgnoreCase) || title.Equals(needle, StringComparison.OrdinalIgnoreCase);
            // 运行请求可以传用户原话；血斛、清心等两字资源名也必须能命中。
            var named = exact || name.Length >= 2 && needle.Contains(name, StringComparison.OrdinalIgnoreCase)
                || title.Length >= 2 && needle.Contains(title, StringComparison.OrdinalIgnoreCase);
            var score = exact ? 140 : named ? 100 : name.Contains(needle, StringComparison.OrdinalIgnoreCase) ? 60
                : description.Contains(needle, StringComparison.OrdinalIgnoreCase) ? 20 : 0;
            if (node.TryGetProperty("tags", out var tags) && tags.ToString().Contains(needle, StringComparison.OrdinalIgnoreCase)) score += 10;
            var pathing = path.StartsWith("pathing/", StringComparison.Ordinal);
            var directory = node.GetProperty("type").GetString() == "directory";
            var selected = parent.Length > 0 && score > 0 && (!pathing || directory && !covered);
            if (selected) matches.Add((score, path, node, named));
            if (node.TryGetProperty("children", out var children))
                foreach (var child in children.EnumerateArray()) Visit(child, path, covered || pathing && selected);
        }
        foreach (var node in document.RootElement.GetProperty("indexes").EnumerateArray()) Visit(node, "", false);
        var ordered = matches.OrderByDescending(match => match.Score).ThenBy(match => match.Path, StringComparer.Ordinal).ToArray();
        var items = ordered.Skip(offset).Take(limit).Select(match => new
        {
            path = match.Path, category = match.Path.Split('/')[0],
            name = match.Node.GetProperty("name").GetString(), type = match.Node.GetProperty("type").GetString(),
            description = Description(match.Node),
            version = match.Node.TryGetProperty("version", out var version) ? version.GetString() : null,
            namedInQuery = match.Named,
            executionScope = match.Path.StartsWith("pathing/") ? "directoryRecursive" : "script",
            requirements = Requirements(match.Node),
            children = match.Node.TryGetProperty("children", out var children) ? children.EnumerateArray().Select(child => new {
                path = match.Path + "/" + child.GetProperty("name").GetString(), name = child.GetProperty("name").GetString(),
                type = child.GetProperty("type").GetString(), description = Description(child), requirements = Requirements(child),
            }).Take(30).ToArray() : null,
        }).ToArray();
        var pathingFound = items.Any(item => item.category == "pathing");
        return new
        {
            source = "centralRepository", category, query = needle, total = ordered.Length, offset,
            nextOffset = offset + items.Length < ordered.Length ? (int?)(offset + items.Length) : null,
            repositoryTime = document.RootElement.TryGetProperty("time", out var time) ? time.GetString() : null, items,
            next = pathingFound
                ? "已找到地图追踪父节点。存在作者包时选择一个完整目录并核对 requirements。直接 describe/invoke bgi.subscribe_script_resources 订阅，bgi.prepare_pathing_group 生成配置组。不要重写叶子 JSON，不搜索 JS manifest 或扫描桥程序集。游戏就绪后直接 describe/invoke bgi.run_script_group。"
                : items.Length > 0
                    ? "使用精确 path 读取对应文档；JS 才读取 manifest、settings 和入口源码。未订阅也可阅读。"
                    : category == "js"
                        ? "仅 JS 分类未命中，不代表地图追踪没有资源。采集或路线问题应搜索 category=pathing 或 all。"
                        : "当前分类索引未命中，核对名称和分类；不要把未订阅或某一分类无结果当成整个仓库不存在。",
        };
    }

    public static object Read(string path, string content, int startLine, int maxLines, string? contains)
    {
        path = NormalizePath(path);
        if (startLine < 1 || maxLines < 1 || maxLines > 240)
            throw BridgeException.InvalidArgument("startLine 从 1 开始，maxLines 必须在 1 到 240 之间。");
        contains = string.IsNullOrEmpty(contains) ? null : contains;
        if (content.Contains('\0')) throw BridgeException.InvalidArgument("该文件不是可读取的文本。");
        var lines = content.Replace("\r\n", "\n").Replace('\r', '\n').Split('\n');
        var selected = new SortedSet<int>();
        var matched = new List<int>();
        if (!string.IsNullOrEmpty(contains))
        {
            for (var i = startLine - 1; i < lines.Length; i++)
                if (lines[i].Contains(contains, StringComparison.OrdinalIgnoreCase)) matched.Add(i);
            foreach (var hit in matched)
            {
                if (selected.Count >= maxLines) break;
                for (var i = Math.Max(startLine - 1, hit - 6); i <= Math.Min(lines.Length - 1, hit + 12) && selected.Count < maxLines; i++)
                    selected.Add(i);
            }
        }
        else
            for (var i = startLine - 1; i < lines.Length && selected.Count < maxLines; i++) selected.Add(i);
        var output = new List<object>();
        var chars = 0;
        var last = startLine - 2;
        foreach (var i in selected)
        {
            if (chars + lines[i].Length > 24000 && output.Count > 0) break;
            if (lines[i].Length > 24000)
                throw BridgeException.InvalidArgument("单行文本超过 24000 字符，无法完整返回；请选择其他文本文件。");
            output.Add(new { line = i + 1, text = lines[i] });
            chars += lines[i].Length;
            last = i;
        }
        var truncated = contains is null
            ? last + 1 < lines.Length
            : matched.Any(hit => hit > last) || selected.Any(i => i > last);
        return new
        {
            source = "centralRepository", path, totalLines = lines.Length,
            sha256 = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(content))).ToLowerInvariant(),
            startLine, contains, matchCount = matched.Count, matchedLines = matched.Take(240).Select(i => i + 1).ToArray(),
            lines = output, truncated,
            nextLine = truncated ? (int?)(last + 2) : null,
        };
    }
}
