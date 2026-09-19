using System.Globalization;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Tools;

/// <summary>
/// 宿主自己写的运行日志：<c>&lt;安装目录&gt;\log\better-genshin-impact&lt;yyyyMMdd&gt;.log</c>，
/// 含脚本 log() 输出、脚本异常与宿主异常。
/// </summary>
public static class HostLogTools
{
    public const string Group = "lifecycle";

    private const string FilePrefix = "better-genshin-impact";
    private const string FileSuffix = ".log";

    /// <summary>宿主记录脚本失败用的固定文本；带消息与带异常转储两种写法都以它开头。</summary>
    private const string FailureAnchor = "执行脚本时发生异常";

    /// <summary>单次最多读取文件尾部的字节数。</summary>
    private const int MaxReadBytes = 4 * 1024 * 1024;

    /// <summary>同一线程上离失败锚点超过这个秒数的记录，不算那次失败的上下文。</summary>
    private const int WindowSeconds = 15;

    /// <summary>两次失败的消息相同且间隔不超过这个秒数，视为同一次失败的重复记录。</summary>
    private const int DuplicateSeconds = 3;

    private const int MaxBodyChars = 4000;
    private const int MaxMessageChars = 600;
    private const int MaxContextEntries = 8;
    private const int MaxContextChars = 300;
    private const int MaxContextSourceChars = 2000;
    private const int MaxLocations = 8;

    /// <summary>ClearScript 转储里混着压缩过的引擎源码，超过这个长度的行一律不算 JS 错误。</summary>
    private const int MaxJsErrorLineChars = 400;

    private static string LogDirectory =>
        Path.Combine(AppContext.BaseDirectory.TrimEnd(Path.DirectorySeparatorChar), "log");

    /// <summary>记录头：<c>[HH:MM:SS.mmm] [LVL] [Primary:S4:P&lt;pid&gt;:T&lt;tid&gt;] &lt;LoggerName&gt;</c>，消息在后续行。</summary>
    private static readonly Regex Header = new(
        @"^\[(?<time>\d{2}:\d{2}:\d{2}\.\d{3})\]\s+\[(?<level>[A-Z]+)\]\s+\[(?<thread>[^\]]*)\]\s*(?<tail>.*)$",
        RegexOptions.Compiled | RegexOptions.CultureInvariant);

    /// <summary>宿主栈帧，形如 <c>at A.B.C(...) in D:\a\...\ScriptService.cs:line 346</c>。</summary>
    private static readonly Regex HostFrame = new(
        @"\bin (?<path>[A-Za-z]:\\[^\r\n:]+?\.\w+):line (?<line>\d+)",
        RegexOptions.Compiled | RegexOptions.CultureInvariant);

    /// <summary>单引号里的绝对路径，形如 <c>'D:\BetterGI\User\AutoPathing\...\01.json'</c>。</summary>
    private static readonly Regex AbsolutePath = new(
        @"'(?<path>[A-Za-z]:\\[^'\r\n]+?\.\w+)'",
        RegexOptions.Compiled | RegexOptions.CultureInvariant);

    private static readonly Regex ScriptFinished = new(
        @"脚本执行结束[:：]\s*""(?<name>[^""]+)""",
        RegexOptions.Compiled | RegexOptions.CultureInvariant);

    /// <summary>
    /// 异常的完全限定类型名前缀，形如 <c>System.IO.DirectoryNotFoundException: </c>。
    /// 宿主把同一次失败写两遍，一条带前缀一条不带，剥掉前缀后两条消息才可比。
    /// </summary>
    private static readonly Regex ExceptionPrefix = new(
        @"^(?:[\w`]+\.)*[\w`]+Exception: ",
        RegexOptions.Compiled | RegexOptions.CultureInvariant);

    public static void Register(MethodRegistry registry)
    {
        var read = new AgentGuide(
            "宿主运行日志",
            "读取 BetterGI 自己写的按天日志。脚本的 log() 输出、宿主异常和脚本异常都在里面，不需要用户复制粘贴。",
            ["用户描述的现象需要看宿主内部过程时，例如脚本失败、启动失败、界面命令报错。", "已经拿到报错原文，需要确认它出现在哪一次运行、哪个脚本、哪个宿主文件。"],
            ["桥已连接。", "只读取桥所在 BetterGI 安装目录下的 log 目录；日志按天分文件。"],
            ["无写入副作用。"],
            "entries 按时间升序，是读取范围内命中筛选条件的记录。headTruncated=true 表示文件较大，只读了尾部，更早的记录没有读到。exists=false 表示这一天没有日志文件，此时看 availableDates 换日期。",
            "改变 date、level、logger、contains 或 thread 重新读取；重复同一条查询不算新证据。",
            "只读。",
            [JsonSerializer.SerializeToElement(new { logger = "ScriptService", level = "ERR", limit = 20 })]);
        registry.Register(
            "bgi.read_host_log",
            Group,
            read.Purpose,
            ReadHostLog,
            inputSchema: AgentSchemas.Input("bgi.read_host_log"),
            guide: read);

        var errors = new AgentGuide(
            "脚本报错",
            "从宿主日志里提取脚本执行失败：报错原文、JS 层错误、出错的脚本名、涉及的用户文件路径和宿主源位置。",
            ["用户报告脚本报错、任务中途失败，或要求排查脚本问题时，先用它拿到确切报错，再去读脚本源码。", "需要确认一个报错属于哪个脚本、哪一次运行时。"],
            ["桥已连接。", "失败发生在当天或用户指定的日期；日志按天分文件。"],
            ["无写入副作用。"],
            "failures 按时间升序，只包含宿主记录了「执行脚本时发生异常」的失败。error 是宿主给出的一手报错原文；jsError 非空表示错误由 JS 引擎抛出，否则来自宿主或用户资源。script 是从同一次运行的日志里读到的脚本名，可能为 null。locations 里 kind=userFile 的路径相对 User 目录，可直接用 bgi.user.read 读取；kind=hostSource 是宿主源码位置，与接口说明里的 source 字段同一种写法；kind=absolute 是报错原文里的其他绝对路径，可能来自打包机而不在本机。count=0 表示该日期没有脚本失败记录。",
            "按 script 读取 User/JsScript 下的源码核对 error 指出的位置；需要完整过程时用 bgi.read_host_log 按同一 thread 读取那次运行。",
            "只读。",
            [JsonSerializer.SerializeToElement(new { limit = 3 })]);
        registry.Register(
            "bgi.get_script_errors",
            Group,
            errors.Purpose,
            ScriptErrors,
            inputSchema: AgentSchemas.Input("bgi.get_script_errors"),
            guide: errors);
    }

    // ---------- 读取 ----------

    private static Task<object?> ReadHostLog(JsonElement arguments, CancellationToken cancellation)
    {
        var date = ResolveDate(arguments);
        var file = Path.Combine(LogDirectory, $"{FilePrefix}{date}{FileSuffix}");
        var availableDates = AvailableDates();
        if (!File.Exists(file))
            return Task.FromResult<object?>(new
            {
                date,
                exists = false,
                file,
                entriesSeen = 0,
                matched = 0,
                headTruncated = false,
                entries = Array.Empty<object>(),
                availableDates,
                note = "这一天没有宿主日志文件。用户当天没有启动过 BetterGI 时也不会生成；可用日期见 availableDates。",
            });

        var (text, size, truncated) = ReadTail(file, MaxReadBytes);
        cancellation.ThrowIfCancellationRequested();
        var entries = Parse(text);

        var level = Text(arguments, "level");
        var logger = Text(arguments, "logger");
        var contains = Text(arguments, "contains");
        var thread = Text(arguments, "thread");
        var limit = Limit(arguments, 30, 100);

        var matched = entries.Where(entry => Matches(entry, level, logger, contains, thread)).ToArray();
        var returned = matched.TakeLast(limit).Select(Project).ToArray();
        return Task.FromResult<object?>(new
        {
            date,
            exists = true,
            file,
            fileBytes = size,
            scannedBytes = Math.Min(size, MaxReadBytes),
            headTruncated = truncated,
            entriesSeen = entries.Count,
            matched = matched.Length,
            entries = returned,
            availableDates,
            note = matched.Length > returned.Length
                ? $"只返回最近 {returned.Length} 条。用 level、logger、contains 或 thread 缩小范围再看更早的记录。"
                : null,
        });
    }

    private static Task<object?> ScriptErrors(JsonElement arguments, CancellationToken cancellation)
    {
        var date = ResolveDate(arguments);
        var file = Path.Combine(LogDirectory, $"{FilePrefix}{date}{FileSuffix}");
        var availableDates = AvailableDates();
        if (!File.Exists(file))
            return Task.FromResult<object?>(new
            {
                date,
                exists = false,
                file,
                count = 0,
                failures = Array.Empty<object>(),
                availableDates,
                note = "这一天没有宿主日志文件，因此没有脚本失败记录；可用日期见 availableDates。",
            });

        var (text, size, truncated) = ReadTail(file, MaxReadBytes);
        cancellation.ThrowIfCancellationRequested();
        var failures = Failures(Parse(text));
        var limit = Limit(arguments, 3, 20);
        var returned = failures.TakeLast(limit).ToArray();
        var note = (failures.Count, returned.Length) switch
        {
            (0, _) => "该日期的宿主日志里没有记录脚本失败。确认日期是否正确，或让用户重新执行一次出错的脚本。",
            var (total, shown) when total > shown => $"只返回最近 {shown} 次失败。",
            _ => null,
        };
        return Task.FromResult<object?>(new
        {
            date,
            exists = true,
            file,
            fileBytes = size,
            headTruncated = truncated,
            count = failures.Count,
            failures = returned.Select(Project).ToArray(),
            availableDates,
            note,
        });
    }

    // ---------- 日志解析 ----------

    /// <summary>一条记录：时间、级别、线程、记录器与消息正文（含续行）。</summary>
    public sealed record HostLogEntry(TimeSpan Time, string Level, string Thread, string Logger, string Body);

    /// <summary>从同一次运行里提取出的脚本失败。</summary>
    public sealed record ScriptFailure(
        TimeSpan Time, string Level, string Thread, string? Script, string? Error, string? JsError,
        IReadOnlyList<LogLocation> Locations, IReadOnlyList<HostLogEntry> Context);

    /// <summary>报错指向的位置。kind=userFile 的 path 相对 User 目录，kind=hostSource 是宿主源码。</summary>
    public sealed record LogLocation(string Kind, string Path, int? Line);

    /// <summary>解析日志正文。记录头开新记录，其余行归入当前记录；文件尾部截断时开头的半行会被丢弃。</summary>
    public static List<HostLogEntry> Parse(string text)
    {
        var entries = new List<HostLogEntry>();
        var (time, level, thread, logger) = (default(TimeSpan), "", "", "");
        var body = new StringBuilder();
        var open = false;

        void Flush()
        {
            if (!open) return;
            entries.Add(new HostLogEntry(time, level, thread, logger, body.ToString().Trim()));
            body.Clear();
        }

        foreach (var raw in text.Split('\n'))
        {
            var line = raw.TrimEnd('\r');
            var header = Header.Match(line);
            if (header.Success)
            {
                Flush();
                open = true;
                TimeSpan.TryParse(header.Groups["time"].Value, CultureInfo.InvariantCulture, out time);
                level = header.Groups["level"].Value;
                thread = ShortThread(header.Groups["thread"].Value);
                var tail = header.Groups["tail"].Value.Trim();
                // 记录器名和消息可能在同一行，也可能消息从下一行开始。
                var gap = tail.IndexOf(' ');
                var head = gap < 0 ? tail : tail[..gap];
                if (head.Contains('.', StringComparison.Ordinal))
                {
                    logger = head;
                    if (gap >= 0) body.Append(tail[(gap + 1)..]);
                }
                else
                {
                    logger = "";
                    body.Append(tail);
                }
                continue;
            }
            if (!open) continue;
            if (body.Length > 0) body.Append('\n');
            body.Append(line);
        }
        Flush();
        return entries;
    }

    /// <summary>提取脚本失败。锚点是宿主固定写的失败文本，上下文取同线程、邻近的记录。</summary>
    public static List<ScriptFailure> Failures(List<HostLogEntry> entries)
    {
        var anchors = new List<int>();
        for (var index = 0; index < entries.Count; index++)
            if (entries[index].Body.Contains(FailureAnchor, StringComparison.Ordinal)) anchors.Add(index);

        var failures = anchors
            .Select(index => Build(entries[index], Window(entries, anchors, index)))
            .ToList();
        return Merge(failures);
    }

    private static ScriptFailure Build(HostLogEntry anchor, List<HostLogEntry> window) => new(
        anchor.Time,
        anchor.Level,
        anchor.Thread,
        Script(anchor, window),
        Error(anchor),
        JsError(anchor, window),
        Locations(anchor, window),
        Context(anchor, window));

    /// <summary>锚点之间、同一线程、时间邻近的记录。</summary>
    private static List<HostLogEntry> Window(List<HostLogEntry> entries, List<int> anchors, int index)
    {
        var anchor = entries[index];
        var order = anchors.IndexOf(index);
        var previous = order > 0 && entries[anchors[order - 1]].Thread == anchor.Thread ? anchors[order - 1] : -1;
        var next = order + 1 < anchors.Count && entries[anchors[order + 1]].Thread == anchor.Thread
            ? anchors[order + 1]
            : entries.Count;

        var window = new List<HostLogEntry>();
        for (var position = previous + 1; position < next; position++)
        {
            if (position == index) continue;
            var candidate = entries[position];
            if (candidate.Thread != anchor.Thread) continue;
            if (Math.Abs((candidate.Time - anchor.Time).TotalSeconds) > WindowSeconds) continue;
            window.Add(candidate);
        }
        return window;
    }

    /// <summary>
    /// 归并同一次失败的重复记录：宿主先写一条带异常转储的，再写一条只有消息的，
    /// 两者时间接近、消息相同（只是一个带类型名前缀）。
    /// </summary>
    private static List<ScriptFailure> Merge(List<ScriptFailure> failures)
    {
        var merged = new List<ScriptFailure>();
        foreach (var failure in failures)
        {
            var duplicate = -1;
            for (var index = merged.Count - 1; index >= 0; index--)
            {
                var candidate = merged[index];
                if (candidate.Thread != failure.Thread) continue;
                if ((failure.Time - candidate.Time).TotalSeconds > DuplicateSeconds) break;
                if (candidate.Error != failure.Error) continue;
                duplicate = index;
                break;
            }
            if (duplicate < 0)
            {
                merged.Add(failure);
                continue;
            }
            var previous = merged[duplicate];
            merged[duplicate] = previous with
            {
                Script = previous.Script ?? failure.Script,
                JsError = previous.JsError ?? failure.JsError,
                Locations = previous.Locations
                    .Concat(failure.Locations)
                    .Distinct()
                    .Take(MaxLocations)
                    .ToArray(),
                Context = previous.Context
                    .Concat(failure.Context)
                    .Distinct()
                    .OrderBy(entry => entry.Time)
                    .Take(MaxContextEntries)
                    .ToArray(),
            };
        }
        return merged;
    }

    /// <summary>报错原文：锚点行带消息就直接取，否则取下一行的异常文本。</summary>
    private static string? Error(HostLogEntry anchor)
    {
        var lines = Lines(anchor.Body);
        var start = Array.FindIndex(lines, line => line.Contains(FailureAnchor, StringComparison.Ordinal));
        if (start < 0) return null;

        var head = lines[start];
        var offset = head.IndexOf(FailureAnchor, StringComparison.Ordinal) + FailureAnchor.Length;
        var rest = head[offset..].TrimStart(':', '：', ' ', '\t').Trim();
        if (rest.Length == 0 && start + 1 < lines.Length) rest = lines[start + 1];
        if (rest.Length == 0) return null;
        return Clip(ExceptionPrefix.Replace(Unquote(rest), ""), MaxMessageChars);
    }

    /// <summary>JS 引擎抛出的错误行，形如 <c>Error: A task was canceled.</c>。</summary>
    private static string? JsError(HostLogEntry anchor, List<HostLogEntry> window)
    {
        foreach (var entry in new[] { anchor }.Concat(window))
            foreach (var line in Lines(entry.Body))
            {
                var text = line.Trim();
                if (text.Length is <= 6 or > MaxJsErrorLineChars) continue;
                if (!text.StartsWith("Error:", StringComparison.Ordinal)) continue;
                return Clip(text[6..].Trim(), MaxMessageChars);
            }
        return null;
    }

    /// <summary>同一次运行里宿主写下的脚本名。</summary>
    private static string? Script(HostLogEntry anchor, List<HostLogEntry> window)
    {
        string? name = null;
        var distance = double.MaxValue;
        foreach (var entry in new[] { anchor }.Concat(window))
        {
            var match = ScriptFinished.Match(entry.Body);
            if (!match.Success) continue;
            var delta = Math.Abs((entry.Time - anchor.Time).TotalSeconds);
            if (delta >= distance) continue;
            name = match.Groups["name"].Value;
            distance = delta;
        }
        return name;
    }

    /// <summary>
    /// 报错指向的位置。用户文件排在最前；宿主源码保留仓库相对路径，
    /// 与接口说明里的 source 字段同形。
    /// </summary>
    private static List<LogLocation> Locations(HostLogEntry anchor, List<HostLogEntry> window)
    {
        var found = new List<LogLocation>();
        var userRoot = Path.GetFullPath(Path.Combine(
            AppContext.BaseDirectory.TrimEnd(Path.DirectorySeparatorChar), "User")) + Path.DirectorySeparatorChar;

        foreach (var entry in new[] { anchor }.Concat(window))
        {
            foreach (var line in Lines(entry.Body))
            {
                foreach (Match match in AbsolutePath.Matches(line))
                    Add(found, Classify(match.Groups["path"].Value, null, userRoot));
                foreach (Match match in HostFrame.Matches(line))
                    Add(found, Classify(
                        match.Groups["path"].Value,
                        int.Parse(match.Groups["line"].Value, CultureInfo.InvariantCulture),
                        userRoot));
            }
        }

        return found
            .OrderBy(location => location.Kind == "userFile" ? 0 : 1)
            .Take(MaxLocations)
            .ToList();
    }

    private static LogLocation Classify(string path, int? line, string userRoot)
    {
        if (path.StartsWith(userRoot, StringComparison.OrdinalIgnoreCase))
            return new LogLocation(
                "userFile",
                path[userRoot.Length..].Replace('\\', '/'),
                line);

        // 宿主源码在日志里是构建机的仓库路径 D:\a\better-genshin-impact\better-genshin-impact\...，
        // 截到仓库根为止。
        const string Repository = "better-genshin-impact\\";
        var marker = path.LastIndexOf(Repository, StringComparison.OrdinalIgnoreCase);
        return marker < 0
            ? new LogLocation("absolute", path, line)
            : new LogLocation("hostSource", path[(marker + Repository.Length)..].Replace('\\', '/'), line);
    }

    private static void Add(List<LogLocation> found, LogLocation location)
    {
        if (found.Count >= MaxLocations * 4) return;
        if (!found.Contains(location)) found.Add(location);
    }

    /// <summary>邻近的短记录：脚本自己的 log() 输出、开始与结束行都在这里。</summary>
    private static List<HostLogEntry> Context(HostLogEntry anchor, List<HostLogEntry> window)
    {
        // 异常转储是几百行的另一条记录，不进上下文。
        var candidates = window.Where(entry => entry.Body.Length is > 0 and <= MaxContextSourceChars);
        return candidates
            .OrderBy(entry => Math.Abs((entry.Time - anchor.Time).TotalSeconds))
            .Take(MaxContextEntries)
            .OrderBy(entry => entry.Time)
            .Select(entry => entry with { Body = Clip(entry.Body, MaxContextChars) })
            .ToList();
    }

    // ---------- 文件与筛选 ----------

    /// <summary>
    /// 读文件尾部；按天写的日志可能有几 MB。截断处会切出半行，丢弃第一行后按记录解析。
    /// </summary>
    private static (string Text, long Size, bool Truncated) ReadTail(string path, int maxBytes)
    {
        // 宿主持有写句柄，读取必须允许共享。
        using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
        var size = stream.Length;
        var truncated = size > maxBytes;
        if (truncated) stream.Seek(size - maxBytes, SeekOrigin.Begin);
        using var reader = new StreamReader(stream, Encoding.UTF8, detectEncodingFromByteOrderMarks: true);
        var text = reader.ReadToEnd();
        if (truncated)
        {
            var cut = text.IndexOf('\n');
            if (cut >= 0) text = text[(cut + 1)..];
        }
        return (text, size, truncated);
    }

    /// <summary>本机已有的日志日期，最新在前。</summary>
    private static string[] AvailableDates()
    {
        try
        {
            return Directory.Exists(LogDirectory)
                ? Directory.EnumerateFiles(LogDirectory, FilePrefix + "*" + FileSuffix)
                    .Select(Path.GetFileName)
                    .Where(name => name is { Length: > 0 })
                    .Select(name => name![FilePrefix.Length..^FileSuffix.Length])
                    .Where(date => date.Length == 8 && date.All(char.IsAsciiDigit))
                    .OrderDescending()
                    .Take(10)
                    .ToArray()
                : [];
        }
        catch (IOException)
        {
            return [];
        }
        catch (UnauthorizedAccessException)
        {
            return [];
        }
    }

    private static bool Matches(HostLogEntry entry, string? level, string? logger, string? contains, string? thread) =>
        (level is null || entry.Level.Equals(level, StringComparison.OrdinalIgnoreCase))
        && (logger is null || entry.Logger.Contains(logger, StringComparison.OrdinalIgnoreCase))
        && (contains is null || entry.Body.Contains(contains, StringComparison.OrdinalIgnoreCase))
        && (thread is null || entry.Thread.Equals(thread, StringComparison.OrdinalIgnoreCase)
            || entry.Thread.EndsWith(thread, StringComparison.OrdinalIgnoreCase));

    private static object Project(HostLogEntry entry)
    {
        var clipped = entry.Body.Length > MaxBodyChars;
        return new
        {
            time = entry.Time.ToString(@"hh\:mm\:ss\.fff", CultureInfo.InvariantCulture),
            level = entry.Level,
            thread = entry.Thread,
            logger = entry.Logger,
            message = clipped ? Clip(entry.Body, MaxBodyChars) : entry.Body,
            truncated = clipped,
        };
    }

    private static object Project(ScriptFailure failure) => new
    {
        time = failure.Time.ToString(@"hh\:mm\:ss\.fff", CultureInfo.InvariantCulture),
        level = failure.Level,
        thread = failure.Thread,
        script = failure.Script,
        error = failure.Error,
        jsError = failure.JsError,
        locations = failure.Locations.Select(location => new
        {
            kind = location.Kind,
            path = location.Path,
            line = location.Line,
        }).ToArray(),
        context = failure.Context.Select(entry => new
        {
            time = entry.Time.ToString(@"hh\:mm\:ss\.fff", CultureInfo.InvariantCulture),
            logger = entry.Logger,
            message = entry.Body,
        }).ToArray(),
    };

    /// <summary>线程段是 <c>Primary:S4:P26172:T1788103607157</c>，只有最后一段标识一次运行。</summary>
    private static string ShortThread(string raw)
    {
        var marker = raw.LastIndexOf(':');
        return marker < 0 ? raw : raw[(marker + 1)..];
    }

    private static string[] Lines(string body) =>
        body.Split('\n').Select(line => line.TrimEnd('\r')).Where(line => line.Trim().Length > 0).ToArray();

    private static string Unquote(string text)
    {
        var trimmed = text.Trim().Trim('"').Trim();
        return trimmed;
    }

    /// <summary>按字符截断，不切开代理对。</summary>
    private static string Clip(string text, int max)
    {
        if (text.Length <= max) return text;
        var end = max;
        if (char.IsHighSurrogate(text[end - 1])) end--;
        return text[..end] + "…";
    }

    private static string ResolveDate(JsonElement arguments)
    {
        var raw = Text(arguments, "date");
        if (raw is null) return DateTime.Now.ToString("yyyyMMdd", CultureInfo.InvariantCulture);
        var digits = raw.Replace("-", "", StringComparison.Ordinal).Replace("/", "", StringComparison.Ordinal).Trim();
        return digits.Length == 8 && digits.All(char.IsAsciiDigit)
            ? digits
            : throw BridgeException.InvalidArgument("date 必须是 yyyyMMdd 或 yyyy-MM-dd。");
    }

    private static string? Text(JsonElement arguments, string name) =>
        arguments.ValueKind == JsonValueKind.Object
        && arguments.TryGetProperty(name, out var value)
        && value.ValueKind == JsonValueKind.String
        && value.GetString() is { Length: > 0 } text
            ? text
            : null;

    private static int Limit(JsonElement arguments, int fallback, int max) =>
        arguments.ValueKind == JsonValueKind.Object
        && arguments.TryGetProperty("limit", out var value)
        && value.ValueKind == JsonValueKind.Number
            ? Math.Clamp(value.GetInt32(), 1, max)
            : fallback;
}
