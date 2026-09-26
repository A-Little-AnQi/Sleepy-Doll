using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Threading;
using BgiBridge.Catalog;
using BgiBridge.Protocol;

namespace BgiBridge.Bgi;

/// <summary>只处理本次命令新建的输入窗口；输入来自当前调用，不接管既有窗口。</summary>
public sealed class NativeDialogScope : IDisposable
{
    public static JsonElement Schema => ArgumentSchema.Parse("""{"type":"object","minProperties":1,"properties":{"text":{"type":"string","maxLength":8192},"filePath":{"type":"string","minLength":1,"maxLength":1024},"selectedValues":{"type":"array","minItems":1,"maxItems":512,"items":{"type":"string","maxLength":1024}},"confirm":{"type":"boolean"},"values":{"type":"object","maxProperties":64}},"additionalProperties":false,"description":"本次命令的明确输入。text 用于名称／文本，filePath 用于文件或目录选择，selectedValues 必须是资源返回的原生 Tag 路径或 folderName；confirm 仅确认本次已授权操作；values 是新窗口数据上下文中的已定义字段。"}""");
    private readonly JsonElement? input;
    private readonly string[] titles;
    private readonly HashSet<Window> beforeWindows;
    private readonly HashSet<nint> beforeHandles;
    private readonly HashSet<Window> touched = [];
    private readonly HashSet<nint> touchedNative = [];
    private readonly DispatcherTimer timer;
    private readonly CancellationToken cancellation;
    private readonly DateTimeOffset started = DateTimeOffset.UtcNow;
    private Exception? failure;
    public int Handled { get; private set; }

    public NativeDialogScope(JsonElement? input, string[]? titles, CancellationToken cancellation)
    {
        this.input = input; this.titles = titles ?? []; this.cancellation = cancellation;
        if (input.HasValue) ArgumentSchema.Validate(input.Value, Schema);
        beforeWindows = Application.Current.Windows.Cast<Window>().ToHashSet();
        beforeHandles = Handles().ToHashSet();
        timer = new DispatcherTimer(TimeSpan.FromMilliseconds(60), DispatcherPriority.Background, Tick, Application.Current.Dispatcher);
        if (input.HasValue) timer.Start(); else timer.Stop();
    }

    private bool Has(string name) => input?.TryGetProperty(name, out _) == true;
    private string? Text(string name) => input.HasValue && input.Value.TryGetProperty(name, out var value) ? value.GetString() : null;
    private bool Accept => !input.HasValue || !input.Value.TryGetProperty("confirm", out var confirm) || confirm.GetBoolean();
    public void Verify() { if (failure is not null) throw failure; }

    private void Tick(object? sender, EventArgs args)
    {
        if (!input.HasValue) return;
        foreach (var window in Application.Current.Windows.Cast<Window>().Where(w => !beforeWindows.Contains(w) && w.IsVisible).ToArray())
        {
            if (!(window.GetType().Name.Contains("PromptDialog", StringComparison.Ordinal) || window.GetType().Name.Contains("MessageBox", StringComparison.Ordinal)
                || window.GetType().Name == "ImageEditWindow" || titles.Contains(window.Title, StringComparer.Ordinal))) continue;
            touched.Add(window);
            try
            {
                if (cancellation.IsCancellationRequested || DateTimeOffset.UtcNow - started > TimeSpan.FromSeconds(30))
                { window.Close(); failure = BridgeException.Failed("弹窗输入已取消或超时，未确认操作完成。"); continue; }
                Fill(window);
            }
            catch (Exception error) { failure = Reflect.Root(error); window.Close(); }
        }
        if (!Has("filePath") && !Has("confirm")) return;
        foreach (var handle in Handles().Where(h => !beforeHandles.Contains(h)))
        {
            var caption = Caption(handle);
            if (!titles.Contains(caption, StringComparer.Ordinal) && !(Has("filePath") && caption is "打开" or "Open" or "另存为" or "Save As" or "浏览文件夹" or "Browse For Folder")) continue;
            try
            {
                touchedNative.Add(handle);
                if (cancellation.IsCancellationRequested || DateTimeOffset.UtcNow - started > TimeSpan.FromSeconds(30))
                { PostMessage(handle, 0x0010, 0, 0); failure = BridgeException.Failed("文件选择已取消或超时。"); continue; }
                var root = AutomationElement.FromHandle(handle);
                if (!Has("filePath"))
                {
                    var choice = root.FindFirst(TreeScope.Descendants, new PropertyCondition(AutomationElement.AutomationIdProperty, Accept ? "6" : "7"))
                        ?? root.FindFirst(TreeScope.Descendants, new PropertyCondition(AutomationElement.AutomationIdProperty, Accept ? "1" : "2"));
                    if (choice?.TryGetCurrentPattern(InvokePattern.Pattern, out var choicePattern) == true) { ((InvokePattern)choicePattern!).Invoke(); Handled++; }
                    continue;
                }
                var editors = root.FindAll(TreeScope.Descendants, new PropertyCondition(AutomationElement.ControlTypeProperty, ControlType.Edit)).Cast<AutomationElement>().ToArray();
                var editor = editors.FirstOrDefault(e => e.Current.AutomationId is "1148" or "FileNameControlHost"
                    || e.Current.Name.Contains("文件名", StringComparison.Ordinal) || e.Current.Name.Contains("File name", StringComparison.OrdinalIgnoreCase)
                    || e.Current.Name.Contains("文件夹", StringComparison.Ordinal) || e.Current.Name.Contains("Folder", StringComparison.OrdinalIgnoreCase));
                if (editor is null) continue;
                if (!editor.TryGetCurrentPattern(ValuePattern.Pattern, out var value)) continue;
                ((ValuePattern)value).SetValue(Text("filePath")!);
                var button = root.FindFirst(TreeScope.Descendants, new PropertyCondition(AutomationElement.AutomationIdProperty, Accept ? "1" : "2"));
                if (button?.TryGetCurrentPattern(InvokePattern.Pattern, out var invoke) != true) continue;
                ((InvokePattern)invoke!).Invoke(); Handled++;
            }
            catch (ElementNotAvailableException) { }
            catch (InvalidOperationException) { }
        }
    }

    private void Fill(Window window)
    {
        var elements = Elements(window).ToArray();
        if (Accept && Has("text"))
        {
            var boxes = elements.OfType<TextBox>().Where(box => !box.IsReadOnly && !box.Name.Contains("filter", StringComparison.OrdinalIgnoreCase)
                && !(box.GetType().GetProperty("PlaceholderText")?.GetValue(box) as string ?? "").Contains("搜索", StringComparison.Ordinal)).ToArray();
            if (boxes.Length != 1) throw BridgeException.InvalidArgument("文本输入没有唯一匹配，不能猜测多个字段。");
            boxes[0].Text = Text("text")!;
        }
        if (Accept && Has("selectedValues"))
        {
            var expected = input!.Value.GetProperty("selectedValues").EnumerateArray().Select(v => v.GetString()!).ToHashSet(StringComparer.OrdinalIgnoreCase);
            var matched = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            var toggles = elements.OfType<System.Windows.Controls.Primitives.ToggleButton>().Where(element => element.Tag is string).ToArray();
            foreach (var element in toggles)
                if (element.Tag is string tag && expected.Contains(tag)) matched.Add(tag);
            if (!expected.SetEquals(matched)) throw BridgeException.InvalidArgument("选择项与当前弹窗真实 Tag 不匹配；不能确认空选择。");
            foreach (var element in toggles) element.IsChecked = expected.Contains((string)element.Tag);
        }
        if (Accept && Has("values"))
        {
            var owner = window.DataContext ?? throw BridgeException.InvalidArgument("窗口没有可绑定的数据上下文。");
            var changes = input!.Value.GetProperty("values").EnumerateObject().Select(item =>
            {
                var property = owner.GetType().GetProperty(item.Name);
                if (property?.SetMethod?.IsPublic != true || property.GetMethod?.IsPublic != true || property.GetIndexParameters().Length != 0)
                    throw BridgeException.InvalidArgument("窗口未定义可写字段 " + item.Name);
                return (Property: property, Value: CommandTargets.Argument(item.Value, property.PropertyType), Before: property.GetValue(owner));
            }).ToArray();
            var applied = 0;
            try
            {
                foreach (var change in changes) { applied++; change.Property.SetValue(owner, change.Value); }
            }
            catch
            {
                foreach (var change in changes.Take(applied).Reverse())
                    try { change.Property.SetValue(owner, change.Before); } catch { }
                throw;
            }
        }
        var words = Accept ? new[] { "使用原图", "确定", "确认", "保存", "是", "OK", "Yes" } : new[] { "取消", "否", "Cancel", "No" };
        var buttons = elements.OfType<Button>().Where(button => button.IsEnabled).ToArray();
        var button = words.Select(word => buttons.FirstOrDefault(b => (b.Content?.ToString() ?? "").Replace("_", "").StartsWith(word, StringComparison.OrdinalIgnoreCase))).FirstOrDefault(b => b is not null);
        if (button is null) return;
        button.RaiseEvent(new RoutedEventArgs(System.Windows.Controls.Primitives.ButtonBase.ClickEvent)); Handled++;
    }

    private static IEnumerable<DependencyObject> Elements(DependencyObject root)
    {
        var queue = new Queue<DependencyObject>(); queue.Enqueue(root);
        var visited = new HashSet<DependencyObject>(ReferenceEqualityComparer.Instance);
        while (queue.Count > 0 && visited.Count < 4096)
        {
            var item = queue.Dequeue(); if (!visited.Add(item)) continue; yield return item;
            foreach (var child in LogicalTreeHelper.GetChildren(item).OfType<DependencyObject>()) queue.Enqueue(child);
        }
    }

    private static IReadOnlyList<nint> Handles()
    {
        var result = new List<nint>();
        EnumWindows((handle, _) => { GetWindowThreadProcessId(handle, out var process); if (process == Environment.ProcessId && IsWindowVisible(handle) && ClassName(handle) == "#32770") result.Add(handle); return true; }, 0);
        return result;
    }
    private static string Caption(nint handle) { var text = new StringBuilder(512); GetWindowText(handle, text, text.Capacity); return text.ToString(); }
    private static string ClassName(nint handle) { var text = new StringBuilder(128); GetClassName(handle, text, text.Capacity); return text.ToString(); }
    public void Dispose() { timer.Stop(); timer.Tick -= Tick; }
    private delegate bool EnumCallback(nint handle, nint parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumCallback callback, nint parameter);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(nint handle, out uint process);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(nint handle);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetWindowText(nint handle, StringBuilder text, int count);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetClassName(nint handle, StringBuilder text, int count);
    [DllImport("user32.dll")] private static extern bool PostMessage(nint handle, uint message, nint wParam, nint lParam);
}
