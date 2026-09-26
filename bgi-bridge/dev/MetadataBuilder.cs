using System.Text.Json;
using System.Text.RegularExpressions;
using System.Xml.Linq;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

if (args.Length != 2) throw new ArgumentException("MetadataBuilder <BetterGI source root> <output JSON>");
var root = Path.GetFullPath(args[0]);
var entries = new SortedDictionary<string, object>(StringComparer.Ordinal);
var enumValues = new SortedDictionary<string, string[]>(StringComparer.Ordinal);
var labels = new Dictionary<string, string?>(StringComparer.Ordinal);
var propertyGuides = new Dictionary<string, string?>(StringComparer.Ordinal);
var uiBindings = new Dictionary<string, HashSet<string>>(StringComparer.Ordinal);
void AddPropertyGuide(string key, string caption)
{
    if (string.IsNullOrWhiteSpace(caption)) return;
    if (propertyGuides.TryGetValue(key, out var prior) && prior != caption) propertyGuides[key] = null;
    else propertyGuides.TryAdd(key, caption);
}
foreach (var path in Directory.EnumerateFiles(root, "*.xaml", SearchOption.AllDirectories).Where(Usable))
{
    try
    {
        var xml = XDocument.Load(path);
        var contextAttribute = xml.Root?.Attributes().FirstOrDefault(a => a.Name.LocalName == "DataContext")?.Value;
        var contextMatch = Regex.Match(contextAttribute ?? "", @"Type=(\w+):(\w+)");
        var contextNamespace = contextMatch.Success ? xml.Root!.GetNamespaceOfPrefix(contextMatch.Groups[1].Value)?.NamespaceName : null;
        var ownerHint = contextNamespace?.StartsWith("clr-namespace:") == true
            ? contextNamespace["clr-namespace:".Length..].Split(';')[0] + "." + contextMatch.Groups[2].Value : null;
        // 包含样式 Setter、EnableCommand/CloseCommand 等自定义属性；不读取 XML 注释中的旧控件。
        foreach (var attribute in xml.Descendants().Attributes().Where(a => a.Value.Contains("{Binding", StringComparison.Ordinal)))
        foreach (Match binding in Regex.Matches(attribute.Value, @"\b(\w+Command)\b"))
        {
            var commandKey = binding.Groups[1].Value;
            if (!uiBindings.TryGetValue(commandKey, out var locations)) uiBindings[commandKey] = locations = [];
            locations.Add(Path.GetRelativePath(root, path).Replace('\\', '/'));
        }
        foreach (var element in xml.Descendants())
        foreach (var attribute in element.Attributes().Where(a => a.Name.LocalName is "IsChecked" or "Text" or "Value" or "SelectedItem" or "SelectedValue" or "SelectedIndex" or "Password"))
        {
            var binding = Regex.Match(attribute.Value, @"\{Binding\s+(?:Path=)?(?:\w+\.)*(\w+Config)\.(\w+)(?:[,}\s])");
            if (!binding.Success) continue;
            var key = binding.Groups[1].Value + "." + binding.Groups[2].Value;
            var caption = Caption(element.Attribute("Header")?.Value ?? element.Attribute("Content")?.Value ?? element.Attribute("ToolTip")?.Value);
            if (caption is null)
                foreach (var parent in element.Ancestors().Take(3))
                {
                    var targets = parent.DescendantsAndSelf().Attributes()
                        .Select(a => Regex.Match(a.Value, @"\{Binding\s+(?:Path=)?(?:\w+\.)*(\w+Config)\.(\w+)(?:[,}\s])"))
                        .Where(m => m.Success).Select(m => m.Groups[1].Value + "." + m.Groups[2].Value).Distinct().ToArray();
                    if (targets.Any(target => target != key)) break;
                    var captions = parent.Descendants().Where(e => e.Name.LocalName.EndsWith("TextBlock"))
                        .Select(e => Caption(e.Attribute("Text")?.Value)).Where(s => !string.IsNullOrWhiteSpace(s)).Distinct().ToArray();
                    if (captions.Length > 0) { caption = string.Join("；", captions); break; }
                }
            if (caption is not null) AddPropertyGuide(key, caption);
        }
        foreach (var element in xml.Descendants())
        {
            var command = element.Attributes().FirstOrDefault(a => a.Name.LocalName.EndsWith("Command", StringComparison.Ordinal))?.Value;
            if (command is null) continue;
            var match = Regex.Match(command, @"(?:Path=)?(?:\w+\.)?(\w+Command)");
            if (match.Success)
            {
                var commandKey = match.Groups[1].Value;
                if (!uiBindings.TryGetValue(commandKey, out var locations)) uiBindings[commandKey] = locations = [];
                locations.Add(Path.GetRelativePath(root, path).Replace('\\', '/'));
            }
            var label = Caption(element.Attribute("Content")?.Value ?? element.Attribute("Header")?.Value ?? element.Attribute("ToolTip")?.Value);
            var card = element.Ancestors().FirstOrDefault(a => a.Name.LocalName is "CardControl" or "CardExpander" or "GroupBox");
            var context = card?.Descendants().Where(e => e.Name.LocalName.EndsWith("TextBlock")).Select(e => Caption(e.Attribute("Text")?.Value)).FirstOrDefault(s => !string.IsNullOrWhiteSpace(s));
            if (string.IsNullOrWhiteSpace(label)) label = element.Descendants().Select(e => Caption(e.Attribute("Text")?.Value)).FirstOrDefault(s => !string.IsNullOrWhiteSpace(s));
            if (string.IsNullOrWhiteSpace(label) && context is not null) label = $"响应「{context}」的界面事件";
            else if (context is not null && context != label) label = context + "：" + label;
            if (match.Success && !string.IsNullOrWhiteSpace(label))
            {
                var commandName = match.Groups[1].Value;
                if (ownerHint is not null) labels.TryAdd(ownerHint + "." + commandName, label);
                if (labels.TryGetValue(commandName, out var existing) && existing != label) labels[commandName] = null;
                else labels.TryAdd(commandName, label);
            }
        }
    }
    catch { }
}
var trees = Directory.EnumerateFiles(root, "*.cs", SearchOption.AllDirectories).Where(Usable)
    .Select(path => (Path: path, Tree: CSharpSyntaxTree.ParseText(File.ReadAllText(path)))).ToArray();
var callerText = trees.ToDictionary(pair => Path.GetRelativePath(root, pair.Path).Replace('\\', '/'), pair => pair.Tree.GetRoot().ToString());
var callers = trees.SelectMany(pair => pair.Tree.GetRoot().DescendantNodes().OfType<IdentifierNameSyntax>()
    .Select(node => (Name: node.Identifier.ValueText, Path: Path.GetRelativePath(root, pair.Path).Replace('\\', '/'))))
    .GroupBy(item => item.Name).ToDictionary(group => group.Key, group => group.Select(item => item.Path).Distinct().ToArray());
var internalEvents = new HashSet<string>(StringComparer.Ordinal)
{
    "Activated", "Closing", "Loaded", "WindowSizeChanged", "OverlayLayoutCommitted", "PointClick", "PointHover", "PointRightClick",
    "DropDownChanged", "ConfigDropDownChanged", "CaptureModeDropDownChanged", "StrategyDropDownOpened", "BeginSeek", "UpdateTrackSelection",
};
foreach (var (_, tree) in trees)
{
    foreach (var call in tree.GetRoot().DescendantNodes().OfType<InvocationExpressionSyntax>()
        .Where(call => call.Expression.ToString().StartsWith("OverlayStyleSettingItem.")))
    {
        var arguments = call.ArgumentList.Arguments;
        if (arguments.Count < 4) continue;
        var member = Regex.Match(arguments[1].ToString(), @"nameof\((\w+Config\.\w+)\)");
        if (member.Success && arguments[2].Expression is LiteralExpressionSyntax title && arguments[3].Expression is LiteralExpressionSyntax description)
            propertyGuides[member.Groups[1].Value] = title.Token.ValueText + "：" + description.Token.ValueText;
    }
    foreach (var creation in tree.GetRoot().DescendantNodes().OfType<ObjectCreationExpressionSyntax>()
        .Where(creation => creation.Type.ToString() == "HotKeySettingModel"))
    {
        var arguments = creation.ArgumentList?.Arguments;
        if (arguments is null || arguments.Value.Count < 2 || arguments.Value[0].Expression is not LiteralExpressionSyntax title) continue;
        var member = Regex.Match(arguments.Value[1].ToString(), @"nameof\(Config\.(HotKeyConfig\.\w+)\)");
        if (!member.Success) continue;
        propertyGuides[member.Groups[1].Value] = "触发「" + title.Token.ValueText + "」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。";
        propertyGuides[member.Groups[1].Value + "Type"] = "「" + title.Token.ValueText + "」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。";
    }
}
foreach (var (path, tree) in trees)
{
    foreach (var declaration in tree.GetRoot().DescendantNodes().OfType<BaseTypeDeclarationSyntax>())
    {
        var ns = string.Join('.', declaration.Ancestors().OfType<BaseNamespaceDeclarationSyntax>().Reverse().Select(n => n.Name.ToString()));
        var owner = ns + "." + string.Join('+', declaration.Ancestors().OfType<TypeDeclarationSyntax>().Reverse()
            .Select(type => type.Identifier.ValueText).Append(declaration.Identifier.ValueText));
        if (declaration is EnumDeclarationSyntax enumeration)
        {
            enumValues.TryAdd(enumeration.Identifier.ValueText, enumeration.Members.Select(member => member.Identifier.ValueText).ToArray());
            continue;
        }
        if (declaration is not TypeDeclarationSyntax type) continue;
        foreach (var member in type.Members)
        {
            var attributes = member.AttributeLists.SelectMany(list => list.Attributes).ToArray();
            string? name = null;
            string? valueType = null;
            var kind = "property";
            string? initial = null;
            if (member is FieldDeclarationSyntax field && attributes.Any(a => a.Name.ToString().EndsWith("ObservableProperty")))
            {
                var variable = field.Declaration.Variables.First();
                var raw = variable.Identifier.ValueText.TrimStart('_');
                name = char.ToUpperInvariant(raw[0]) + raw[1..];
                valueType = field.Declaration.Type.ToString();
                initial = variable.Initializer?.Value.ToString();
            }
            else if (member is PropertyDeclarationSyntax property && property.Modifiers.Any(SyntaxKind.PublicKeyword))
            {
                name = property.Identifier.ValueText;
                valueType = property.Type.ToString();
                initial = property.Initializer?.Value.ToString();
            }
            else if (member is MethodDeclarationSyntax method && attributes.Any(a => a.Name.ToString().EndsWith("RelayCommand")))
            {
                name = method.Identifier.ValueText;
                if (name.StartsWith("On") && name.Length > 2 && char.IsUpper(name[2])) name = name[2..];
                if (name.EndsWith("Async")) name = name[..^5];
                name += "Command";
                kind = "command";
                valueType = method.ParameterList.Parameters.FirstOrDefault()?.Type?.ToString();
            }
            else if (member is MethodDeclarationSyntax api && api.Modifiers.Any(SyntaxKind.PublicKeyword)
                && owner.StartsWith("BetterGenshinImpact.Core.Script.Dependence.", StringComparison.Ordinal))
            {
                name = api.Identifier.ValueText + "(" + string.Join(",", api.ParameterList.Parameters.Select(parameter => parameter.Type?.ToString())) + ")";
                kind = "scriptApi";
                valueType = api.ReturnType.ToString();
            }
            if (name is null) continue;
            var summary = Summary(member);
            if (string.IsNullOrWhiteSpace(summary))
                summary = string.Join(" ", member.GetTrailingTrivia().Where(trivia => trivia.IsKind(SyntaxKind.SingleLineCommentTrivia))
                    .Select(trivia => trivia.ToString()[2..].Trim()));
            var description = attributes.FirstOrDefault(a => a.Name.ToString().EndsWith("Description"))?.ArgumentList?.Arguments.FirstOrDefault()?.Expression as LiteralExpressionSyntax;
            if (description?.Token.ValueText is { Length: > 0 } desc) summary = desc;
            var label = kind == "command" ? labels.GetValueOrDefault(owner + "." + name) ?? labels.GetValueOrDefault(name) : null;
            if (string.IsNullOrWhiteSpace(summary) && label is not null) summary = label;
            var documentationSource = "host-source";
            if (string.IsNullOrWhiteSpace(summary) && propertyGuides.GetValueOrDefault(type.Identifier.ValueText + "." + name) is { } fieldGuide)
            {
                summary = fieldGuide;
                documentationSource = "host-ui-binding";
            }
            var range = attributes.FirstOrDefault(a => a.Name.ToString().EndsWith("Range"))?.ArgumentList?.Arguments.Select(a => a.Expression.ToString()).ToArray();
            var key = (kind == "command" ? "C:" : kind == "scriptApi" ? "M:" : "P:") + owner + "." + name;
            var obsolete = attributes.Concat(type.AttributeLists.SelectMany(list => list.Attributes))
                .Any(a => a.Name.ToString() is "Obsolete" or "ObsoleteAttribute" or "System.Obsolete" or "System.ObsoleteAttribute");
            var implemented = member is not MethodDeclarationSyntax implementation || implementation.ExpressionBody is not null || implementation.Body?.Statements.Count > 0;
            var bindings = uiBindings.GetValueOrDefault(name)?.Order().ToArray() ?? [];
            var sourceCallers = member is MethodDeclarationSyntax calledMethod
                ? callers.GetValueOrDefault(calledMethod.Identifier.ValueText, []).Concat(callers.GetValueOrDefault(name, [])).Distinct()
                    .Where(caller => caller == Path.GetRelativePath(root, path).Replace('\\', '/') || callerText[caller].Contains(type.Identifier.ValueText, StringComparison.Ordinal)).Order().ToArray() : [];
            var hasAwait = member is MethodDeclarationSyntax awaitMethod && awaitMethod.DescendantNodes().OfType<AwaitExpressionSyntax>().Any();
            var asyncVoid = member is MethodDeclarationSyntax asyncMethod && asyncMethod.Modifiers.Any(SyntaxKind.AsyncKeyword) && asyncMethod.ReturnType.ToString() == "void";
            var needsDialogInput = member is MethodDeclarationSyntax dialogMethod && (new[] { "PromptDialog.Prompt", "OpenFileDialog", "SaveFileDialog", "FolderBrowserDialog", "PromptDialog.User", "new PromptDialog" }.Any(dialog => dialogMethod.ToString().Contains(dialog, StringComparison.Ordinal)));
            var dialogCalls = member is MethodDeclarationSyntax modalMethod ? modalMethod.DescendantNodes().OfType<InvocationExpressionSyntax>()
                .Select(call => call.Expression.ToString()).Where(call => call.EndsWith("ShowDialog", StringComparison.Ordinal) || call.EndsWith("ShowDialogAsync", StringComparison.Ordinal)).ToArray() : [];
            var unsupportedDialog = kind == "command" && (dialogCalls.Any(call => call.EndsWith("ShowDialogAsync", StringComparison.Ordinal))
                || dialogCalls.Length > 0 && !needsDialogInput && !(type.Identifier.ValueText == "MapPathingViewModel" && name == "OpenSettingsCommand"));
            var retiredOwner = type.Identifier.ValueText is "FormViewModel" or "AutoPickBlackListViewModel" or "AutoPickWhiteListViewModel";
            var exposureReason = obsolete ? "源码标记 Obsolete，已移除公共入口"
                : kind != "command" ? null
                : retiredOwner ? "旧表单类型已无原生界面／业务调用，使用当前黑白名单窗口或资源接口"
                : !implemented ? "空实现或仅注释占位，已移除而不补造业务"
                : internalEvents.Contains(name[..^7]) ? "WPF 生命周期／控件输入事件，不是独立业务接口"
                : asyncVoid && hasAwait ? "async void 含 await，桥不能等待真实终态"
                : unsupportedDialog ? "模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口"
                : bindings.Length == 0 && sourceCallers.Length == 0 ? "没有有效界面绑定或源码调用，未作为当前产品功能发布"
                : null;
            entries[key] = new
            {
                summary,
                documentationSource,
                label,
                kind,
                valueType,
                initial,
                range,
                source = Path.GetRelativePath(root, path).Replace('\\','/'),
                line = tree.GetLineSpan(member.Span).StartLinePosition.Line + 1,
                hasCustomChangeHook = type.Members.OfType<MethodDeclarationSyntax>().Any(m => m.Identifier.ValueText == $"On{name}Changed"),
                hasImplementation = implemented,
                isObsolete = obsolete,
                exposureReason,
                uiBindings = bindings,
                sourceCallers,
                calls = member is MethodDeclarationSyntax bodyMethod ? bodyMethod.DescendantNodes().OfType<InvocationExpressionSyntax>().Select(i => i.Expression.ToString()).Distinct().ToArray() : null,
                jsonIgnore = attributes.Any(a => a.Name.ToString().EndsWith("JsonIgnore")),
                jsonName = (attributes.FirstOrDefault(a => a.Name.ToString().EndsWith("JsonPropertyName"))?.ArgumentList?.Arguments.FirstOrDefault()?.Expression as LiteralExpressionSyntax)?.Token.ValueText,
                isStatic = member.Modifiers.Any(SyntaxKind.StaticKeyword),
                writable = member is FieldDeclarationSyntax || member is PropertyDeclarationSyntax prop && prop.AccessorList?.Accessors.Any(accessor => accessor.IsKind(SyntaxKind.SetAccessorDeclaration) && !accessor.Modifiers.Any(SyntaxKind.PrivateKeyword)) == true,
                parameters = member is MethodDeclarationSyntax commandMethod ? commandMethod.ParameterList.Parameters.Select(parameter => new { name = parameter.Identifier.ValueText, type = parameter.Type?.ToString() }).ToArray() : null,
                needsDialogInput,
                dialogCalls,
                usesSelection = member is MethodDeclarationSyntax selectedMethod && Regex.IsMatch(selectedMethod.ToString(), @"\bSelected\w+"),
                asyncVoid,
                hasAwait,
                dialogTitles = member is MethodDeclarationSyntax titleMethod ? titleMethod.DescendantNodes().OfType<AssignmentExpressionSyntax>()
                    .Where(a => a.Left.ToString() == "Title").Select(a => a.Right).OfType<LiteralExpressionSyntax>().Select(a => a.Token.ValueText)
                    .Concat(titleMethod.DescendantNodes().OfType<InvocationExpressionSyntax>().Where(i => i.Expression.ToString().Contains("PromptDialog", StringComparison.Ordinal)
                        || i.Expression.ToString().Contains("MessageBox", StringComparison.Ordinal)).Select(i => i.ArgumentList.Arguments.ElementAtOrDefault(1)?.Expression)
                        .OfType<LiteralExpressionSyntax>().Select(a => a.Token.ValueText)).Distinct().ToArray() : null,
            };
        }
    }
}
var output = JsonSerializer.Serialize(new { format = 1, entries, enums = enumValues }, new JsonSerializerOptions { WriteIndented = true, Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping });
File.WriteAllText(args[1], output);
Console.WriteLine($"Indexed {entries.Count} members and {enumValues.Count} enums.");

static bool Usable(string path) => !path.Split(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar).Any(part => part is "obj" or "bin" or ".git");
static string? Caption(string? value)
{
    if (value is null) return null;
    if (value.StartsWith("{i18n:T ") && value.EndsWith('}')) return value[8..^1].Trim(' ', '\'', '"');
    return value.StartsWith('{') ? null : value;
}
static string Summary(MemberDeclarationSyntax member)
{
    var leading = member.GetLeadingTrivia().ToFullString();
    var xmlText = string.Join('\n', leading.Split('\n').Where(line => line.TrimStart().StartsWith("///")).Select(line => line.TrimStart()[3..]));
    if (!string.IsNullOrWhiteSpace(xmlText))
        try { return Regex.Replace(XElement.Parse("<doc>" + xmlText + "</doc>").Element("summary")?.Value ?? "", @"\s+", " ").Trim(); } catch { }
    return string.Join(" ", leading.Split('\n').Select(line => line.Trim()).Where(line => line.StartsWith("//") && !line.StartsWith("///")).Select(line => line[2..].Trim())).Trim();
}
