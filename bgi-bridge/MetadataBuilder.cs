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
            var command = element.Attribute("Command")?.Value;
            if (command is null) continue;
            var match = Regex.Match(command, @"(?:Path=)?(?:\w+\.)?(\w+Command)");
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
            var key = (kind == "command" ? "C:" : "P:") + owner + "." + name;
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
                hasImplementation = member is not MethodDeclarationSyntax implementation || implementation.ExpressionBody is not null || implementation.Body?.Statements.Count > 0,
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
