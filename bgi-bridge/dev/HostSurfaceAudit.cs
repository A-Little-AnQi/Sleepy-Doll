using System.Diagnostics;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Xml.Linq;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

/// <summary>独立盘点公共方法、命令和构造器，不把文档存在记为接口已接通。</summary>
internal static class HostSurfaceAudit
{
    internal static void Write(string source, string output)
    {
        var value = Collect(source);
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(output))!);
        File.WriteAllText(output, value.GetRawText());
        Console.WriteLine(JsonSerializer.Serialize(new { types = value.GetProperty("types").EnumerateObject().Count(),
            methods = value.GetProperty("methods").EnumerateObject().Count(), sourceRevision = value.GetProperty("sourceRevision").GetString() }));
    }

    internal static JsonElement Collect(string source)
    {
        var root = Path.GetFullPath(source);
        var project = Path.Combine(root, "BetterGenshinImpact");
        if (!File.Exists(Path.Combine(project, "BetterGenshinImpact.csproj"))) throw new ArgumentException("缺少正式宿主工程");
        var files = Files(root);
        var fingerprintValue = Fingerprint(root);
        var methods = new SortedDictionary<string, object>(StringComparer.Ordinal);
        var types = new SortedDictionary<string, object>(StringComparer.Ordinal);
        var constructorParts = new Dictionary<string, List<object>>(StringComparer.Ordinal);
        var baseParts = new Dictionary<string, List<string>>(StringComparer.Ordinal);
        foreach (var path in files)
        {
            var relative = Path.GetRelativePath(root, path).Replace('\\', '/');
            var bytes = File.ReadAllBytes(path);
            if (Path.GetExtension(path) != ".cs") continue;
            var tree = CSharpSyntaxTree.ParseText(Encoding.UTF8.GetString(bytes), path: path);
            foreach (var declaration in tree.GetRoot().DescendantNodes().OfType<BaseTypeDeclarationSyntax>())
            {
                var ns = string.Join('.', declaration.Ancestors().OfType<BaseNamespaceDeclarationSyntax>()
                    .Reverse().Select(value => value.Name.ToString()));
                if (!ns.StartsWith("BetterGenshinImpact", StringComparison.Ordinal)) continue;
                var owner = ns + "." + string.Join('+', declaration.Ancestors().OfType<TypeDeclarationSyntax>()
                    .Reverse().Select(value => value.Identifier.ValueText + (value.TypeParameterList?.ToString() ?? ""))
                    .Append(declaration.Identifier.ValueText + (declaration is TypeDeclarationSyntax generic ? generic.TypeParameterList?.ToString() ?? "" : "")));
                if (declaration is EnumDeclarationSyntax enumeration)
                {
                    types.TryAdd(owner, new { name = owner, kind = "enum", summary = Summary(declaration), source = relative,
                        line = tree.GetLineSpan(declaration.Span).StartLinePosition.Line + 1,
                        values = enumeration.Members.Select(member => member.Identifier.ValueText).ToArray() });
                    continue;
                }
                if (declaration is not TypeDeclarationSyntax type) continue;
                if (!baseParts.TryGetValue(owner, out var bases)) baseParts[owner] = bases = [];
                foreach (var name in type.BaseList?.Types.Select(value => value.Type.ToString()) ?? [])
                    if (!bases.Contains(name, StringComparer.Ordinal)) bases.Add(name);
                if (!constructorParts.TryGetValue(owner, out var constructors)) constructorParts[owner] = constructors = [];
                constructors.AddRange(type.Members.OfType<ConstructorDeclarationSyntax>()
                    .Where(member => member.Modifiers.Any(SyntaxKind.PublicKeyword))
                    .Select(member => (object)new { parameters = Parameters(member.ParameterList), source = relative,
                        line = tree.GetLineSpan(member.Span).StartLinePosition.Line + 1 }));
                types.TryAdd(owner, new { name = owner, kind = type.Kind().ToString(), summary = Summary(type), source = relative,
                    line = tree.GetLineSpan(type.Span).StartLinePosition.Line + 1,
                    isAbstract = type.Modifiers.Any(SyntaxKind.AbstractKeyword),
                    isStatic = type.Modifiers.Any(SyntaxKind.StaticKeyword), constructors, bases, baseResolution = "sourceSyntaxOnly" });
                foreach (var method in type.Members.OfType<MethodDeclarationSyntax>())
                {
                    var relay = method.AttributeLists.SelectMany(list => list.Attributes)
                        .Any(attribute => attribute.Name.ToString() is "RelayCommand" or "RelayCommandAttribute");
                    var accessible = method.Modifiers.Any(SyntaxKind.PublicKeyword) || type is InterfaceDeclarationSyntax;
                    var explicitInterface = method.ExplicitInterfaceSpecifier?.Name.ToString();
                    if (!accessible && !relay && explicitInterface is null) continue;
                    var name = (explicitInterface is null ? "" : explicitInterface + ".") + method.Identifier.ValueText;
                    var signature = name + (method.TypeParameterList?.ToString() ?? "") + "(" +
                        string.Join(',', method.ParameterList.Parameters.Select(parameter =>
                            (parameter.Modifiers.Count > 0 ? parameter.Modifiers + " " : "") + parameter.Type)) + ")";
                    var id = "M:" + owner + "." + signature;
                    if (methods.ContainsKey(id)) continue;
                    methods.Add(id, new {
                        id, owner, name, summary = Summary(method), returnType = method.ReturnType.ToString(),
                        kind = relay ? "command" : explicitInterface is not null ? "interfaceImplementation" : "publicMethod",
                        isStatic = method.Modifiers.Any(SyntaxKind.StaticKeyword),
                        asyncVoid = method.Modifiers.Any(SyntaxKind.AsyncKeyword) && method.ReturnType.ToString() == "void",
                        parameters = Parameters(method.ParameterList), source = relative,
                        line = tree.GetLineSpan(method.Span).StartLinePosition.Line + 1,
                        hasImplementation = method.ExpressionBody is not null || method.Body?.Statements.Count > 0,
                        calls = method.DescendantNodes().OfType<InvocationExpressionSyntax>()
                            .Select(call => call.Expression.ToString()).Distinct().Order(StringComparer.Ordinal).ToArray(),
                        callResolution = "sourceSyntaxOnly",
                        exposureVerified = false,
                    });
                }
            }
        }
        var revision = Git(root, "rev-parse", "HEAD");
        var changed = Git(root, "status", "--porcelain", "--untracked-files=no");
        if (fingerprintValue != Fingerprint(root)) throw new IOException("宿主源码在盘点期间变化，请重新生成");
        var value = new { format = 1, sourceScope = "BetterGenshinImpact", sourceRevision = revision,
            sourceDirty = changed.Length != 0, sourceTreeHash = fingerprintValue,
            note = "源码方法和调用表达式是审计分母，不是可执行接口，也不证明运行宿主与源码一致。", types, methods };
        return JsonSerializer.SerializeToElement(value, new JsonSerializerOptions { WriteIndented = true,
            Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping });
    }

    internal static string[] Files(string root)
    {
        var projects = new Queue<string>();
        projects.Enqueue(Path.Combine(root, "BetterGenshinImpact", "BetterGenshinImpact.csproj"));
        var visited = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var files = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        while (projects.TryDequeue(out var project))
        {
            project = Path.GetFullPath(project);
            if (!project.StartsWith(root.TrimEnd(Path.DirectorySeparatorChar) + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase))
                throw new IOException("项目引用超出源码范围：" + project);
            if (!visited.Add(project)) continue;
            var directory = Path.GetDirectoryName(project)!;
            var xml = XDocument.Load(project);
            foreach (var reference in xml.Descendants().Where(element => element.Name.LocalName is "ProjectReference" or "Compile"))
            {
                var include = reference.Attribute("Include")?.Value;
                if (include is null) continue;
                if (include.Contains("$(", StringComparison.Ordinal) || include.Contains('*'))
                    throw new IOException("审计尚未解析此项目输入，不能声明覆盖完整：" + include);
                var path = Path.GetFullPath(Path.Combine(directory, include.Replace('\\', Path.DirectorySeparatorChar)));
                if (!path.StartsWith(root.TrimEnd(Path.DirectorySeparatorChar) + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase))
                    throw new IOException("源码输入超出范围：" + path);
                if (reference.Name.LocalName == "ProjectReference") projects.Enqueue(path);
                else files.Add(path);
            }
            foreach (var path in Directory.EnumerateFiles(directory, "*", SearchOption.AllDirectories)
                .Where(path => Path.GetExtension(path) is ".cs" or ".xaml" or ".csproj")
                .Where(path => !Path.GetRelativePath(root, path).Split(Path.DirectorySeparatorChar).Any(part => part is "bin" or "obj" or ".git" or "User"))) files.Add(path);
        }
        return files.OrderBy(path => Path.GetRelativePath(root, path).Replace('\\', '/'), StringComparer.Ordinal).ToArray();
    }

    internal static string Fingerprint(string root)
    {
        using var fingerprint = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        foreach (var path in Files(root))
        {
            fingerprint.AppendData(Encoding.UTF8.GetBytes(Path.GetRelativePath(root, path).Replace('\\', '/') + "\0"));
            fingerprint.AppendData(SHA256.HashData(File.ReadAllBytes(path)));
        }
        return Convert.ToHexString(fingerprint.GetHashAndReset()).ToLowerInvariant();
    }

    private static object[] Parameters(ParameterListSyntax list) => list.Parameters.Select(parameter => (object)new {
        name = parameter.Identifier.ValueText, type = parameter.Type?.ToString(),
        modifier = parameter.Modifiers.ToString(), optional = parameter.Default is not null,
    }).ToArray();

    private static string Summary(SyntaxNode node)
    {
        var xml = string.Join('\n', node.GetLeadingTrivia().ToFullString().Split('\n')
            .Where(line => line.TrimStart().StartsWith("///", StringComparison.Ordinal)).Select(line => line.TrimStart()[3..]));
        if (xml.Length == 0) return "";
        try { return Regex.Replace(XElement.Parse("<doc>" + xml + "</doc>").Element("summary")?.Value ?? "", @"\s+", " ").Trim(); }
        catch (System.Xml.XmlException) { return ""; }
    }

    private static string Git(string root, params string[] arguments)
    {
        var start = new ProcessStartInfo("git") { RedirectStandardOutput = true, RedirectStandardError = true, UseShellExecute = false, CreateNoWindow = true };
        start.ArgumentList.Add("-C"); start.ArgumentList.Add(root);
        foreach (var argument in arguments) start.ArgumentList.Add(argument);
        using var process = Process.Start(start)!;
        var value = process.StandardOutput.ReadToEnd().Trim();
        var error = process.StandardError.ReadToEnd();
        process.WaitForExit();
        if (process.ExitCode != 0) throw new InvalidOperationException(error);
        return value;
    }
}
