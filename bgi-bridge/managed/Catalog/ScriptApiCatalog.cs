using System.Reflection;
using System.Text.Json;
using BgiBridge.Bgi;
using BgiBridge.Protocol;

namespace BgiBridge.Catalog;

/// <summary>仅描述实际注入 JS 的对象、委托、类型及返回对象；从不执行脚本或创建宿主实例。</summary>
public static class ScriptApiCatalog
{
    private static readonly JsonElement[] Bindings = SourceDocumentation.ScriptBindings.EnumerateArray().Select(b => b.Clone()).ToArray();
    private static readonly HashSet<string> KnownTypes = Bindings.Select(b => b.GetProperty("clrType").GetString()!).Concat(SourceDocumentation.ScriptTypes.EnumerateObject().Select(p => p.Name)).ToHashSet(StringComparer.Ordinal);
    private static readonly object Gate = new();
    private static string Text(JsonElement value, string name) => value.TryGetProperty(name, out var member) && member.ValueKind == JsonValueKind.String ? member.GetString()! : "";
    private static Type? TypeOf(string name) => Reflect.FindType(name);
    public static object Search(string query, int offset, int limit)
    {
        var items = new List<object>();
        foreach (var binding in Bindings)
        {
            var alias = Text(binding,"alias"); var clr = Text(binding,"clrType");
            if ((alias + " " + clr + " " + Text(binding,"member")).Contains(query, StringComparison.OrdinalIgnoreCase))
                items.Add(new { id = alias, kind = Text(binding,"kind"), clrType = clr, source = Text(binding,"source"), usage = BindingUsage(binding) });
        }
        foreach (var type in SourceDocumentation.ScriptTypes.EnumerateObject())
        foreach (var member in type.Value.GetProperty("members").EnumerateArray())
        {
            var name = Text(member,"name");
            if (!(type.Name + " " + name + " " + Text(member,"summary")).Contains(query,StringComparison.OrdinalIgnoreCase)) continue;
            items.Add(new { id = type.Name + "." + name, kind = Text(member,"kind"), clrType = type.Name, summary = Text(member,"summary"), declaration = Text(member,"returnType"), source = Text(member,"source"), line = member.GetProperty("line").GetInt32() });
        }
        if (query.StartsWith("OpenCvSharp.", StringComparison.Ordinal))
        {
            var type = TypeOf(query);
            if (type is not null) items.Add(new { id=query,kind="type",clrType=query,usage="OpenCvSharp 命名空间的真实导出类型；read 分页获取成员。" });
        }
        return new { total=items.Count, items=items.Skip(offset).Take(limit), nextOffset=offset+limit<items.Count?(int?)(offset+limit):null,
            note="这是 JS 运行环境契约，不是可直接 api.invoke 的动作。read 读取类型、成员、重载与继承方法。" };
    }

    public static object Read(string id, string member, int offset, int limit)
    {
        var binding = Bindings.FirstOrDefault(b => Text(b,"alias")==id);
        var clr = binding.ValueKind == JsonValueKind.Undefined ? id : Text(binding,"clrType");
        if(binding.ValueKind==JsonValueKind.Undefined)
        {
            var owner=SourceDocumentation.ScriptTypes.EnumerateObject().Select(p=>p.Name).Where(name=>id.StartsWith(name+".",StringComparison.Ordinal)).OrderByDescending(name=>name.Length).FirstOrDefault();
            if(owner is not null){clr=owner;if(member.Length==0)member=id[(owner.Length+1)..];}
        }
        if (binding.ValueKind != JsonValueKind.Undefined && Text(binding,"kind") == "namespace")
        {
            var assembly = AppDomain.CurrentDomain.GetAssemblies().FirstOrDefault(a=>a.GetName().Name==clr);
            var types = assembly?.GetExportedTypes().Where(t=>t.IsPublic && (member.Length==0||t.FullName!.Contains(member,StringComparison.OrdinalIgnoreCase))).OrderBy(t=>t.FullName).ToArray() ?? [];
            return new { id,kind="namespace",assembly=clr,total=types.Length,types=types.Skip(offset).Take(limit).Select(t=>new{id=t.FullName,kind=t.IsEnum?"enum":"type"}),nextOffset=offset+limit<types.Length?(int?)(offset+limit):null,usage="使用 OpenCvSharp.完整命名空间.类型；查询具体类型读取构造器／静态与实例成员。" };
        }
        lock(Gate)
        {
            if (!KnownTypes.Contains(clr) && !clr.StartsWith("OpenCvSharp.",StringComparison.Ordinal)) throw BridgeException.NotFound("类型不属于已注册 JS 环境或其返回／参数类型。");
        }
        if (binding.ValueKind != JsonValueKind.Undefined && Text(binding,"kind") == "dynamic")
            return new { id,kind="dynamic",usage="settings 是 manifest.settings_ui 对应的当前用户参数；先 inspect_script/read 取得定义，不能从 CLR 固定字段猜测。" };
        var delegateMember = binding.ValueKind != JsonValueKind.Undefined ? Text(binding,"member") : "";
        if (delegateMember.Length>0) member=delegateMember;
        var type = TypeOf(clr);
        if (type is null) return SourceRead(id,clr,member,offset,limit,"当前进程未加载所需依赖；保留当前源码声明，不伪造运行时类型检查成功。");
        try{return RuntimeRead(id,clr,member,offset,limit,binding,type);}
        catch(Exception error) when(error is System.IO.FileNotFoundException or TypeLoadException or ReflectionTypeLoadException or System.IO.FileLoadException)
        {return SourceRead(id,clr,member,offset,limit,"依赖加载限制："+error.GetType().Name+"；保留当前源码契约，未确认该运行时依赖。");}
    }
    private static object RuntimeRead(string id,string clr,string member,int offset,int limit,JsonElement binding,Type type)
    {
        var records = new List<object>();
        foreach (var constructor in type.GetConstructors(BindingFlags.Public|BindingFlags.Instance))
            if (member.Length==0 || member==".ctor") records.Add(Describe(constructor,type.Name,"constructor",clr));
        foreach (var method in type.GetMethods(BindingFlags.Public|BindingFlags.Static|BindingFlags.Instance).Where(m=>!m.IsSpecialName && m.DeclaringType!=typeof(object)))
            if (member.Length==0 || method.Name.Contains(member,StringComparison.OrdinalIgnoreCase)) records.Add(Describe(method,method.Name,"method",clr));
        foreach (var property in type.GetProperties(BindingFlags.Public|BindingFlags.Static|BindingFlags.Instance))
            if (member.Length==0 || property.Name.Contains(member,StringComparison.OrdinalIgnoreCase))
            {
                Remember(property.PropertyType);
                records.Add(new { name=property.Name,kind="property",returnType=Friendly(property.PropertyType),readable=property.GetMethod?.IsPublic==true,writable=property.SetMethod?.IsPublic==true,
                    isStatic=(property.GetMethod??property.SetMethod)?.IsStatic,declaringType=property.DeclaringType?.FullName,indexParameters=property.GetIndexParameters().Select(Parameter),source=Source(property.DeclaringType!,property.Name,"P") });
            }
        foreach (var field in type.GetFields(BindingFlags.Public|BindingFlags.Static|BindingFlags.Instance))
            if (member.Length==0 || field.Name.Contains(member,StringComparison.OrdinalIgnoreCase))
            { Remember(field.FieldType); records.Add(new { name=field.Name,kind="field",returnType=Friendly(field.FieldType),isStatic=field.IsStatic,writable=!field.IsInitOnly&&!field.IsLiteral }); }
        return new { id,clrType=type.FullName,kind=type.IsEnum?"enum":"type",runtimeVerified=true,total=records.Count,members=records.Skip(offset).Take(limit),
            nextOffset=offset+limit<records.Count?(int?)(offset+limit):null,enumValues=type.IsEnum?Enum.GetNames(type):null,
            binding=binding.ValueKind==JsonValueKind.Undefined?(object?)null:binding, usage=binding.ValueKind==JsonValueKind.Undefined?"实例成员通过返回对象访问；只有已注入类型或 OpenCvSharp 导出类型才有全局构造名。":BindingUsage(binding),
            conventions="成员名绑定不区分大小写；Task 自动转 Promise，需要 await。重载保留真实参数、可选默认值、ref/out 与 params；图像区域用完 Dispose，不将已释放对象继续用于 OCR／点击。" };
    }
    private static object SourceRead(string id,string clr,string member,int offset,int limit,string note)
    {
        var records = new List<JsonElement>(); var pending = new Queue<string>(); pending.Enqueue(clr);var seen=new HashSet<string>();
        while(pending.TryDequeue(out var name))
        {
            if(!seen.Add(name)||!SourceDocumentation.ScriptTypes.TryGetProperty(name,out var declaration))continue;
            records.AddRange(declaration.GetProperty("members").EnumerateArray().Where(m=>member.Length==0||Text(m,"name").Contains(member,StringComparison.OrdinalIgnoreCase)).Select(m=>m.Clone()));
            if(declaration.TryGetProperty("bases",out var bases)&&bases.ValueKind==JsonValueKind.Array)foreach(var parent in bases.EnumerateArray())pending.Enqueue(parent.GetString()!);
        }
        return new { id,clrType=clr,runtimeVerified=false,total=records.Count,members=records.Skip(offset).Take(limit),nextOffset=offset+limit<records.Count?(int?)(offset+limit):null,note };
    }
    private static object Describe(MethodBase method,string name,string kind,string clr)
    {
        foreach(var parameter in method.GetParameters())Remember(parameter.ParameterType);
        var returns=(method as MethodInfo)?.ReturnType; if(returns is not null)Remember(returns);
        return new { name,kind,declaringType=method.DeclaringType?.FullName,isStatic=method.IsStatic,parameters=method.GetParameters().Select(Parameter),
            returnType=returns is null?clr:Friendly(returns),awaitRequired=returns is not null&&typeof(Task).IsAssignableFrom(returns),
            deprecated=method.IsDefined(typeof(ObsoleteAttribute),true),source=Source(method.DeclaringType!,name,"M") };
    }
    private static object Parameter(ParameterInfo parameter)=> new { name=parameter.Name,type=Friendly(parameter.ParameterType),optional=parameter.IsOptional,
        defaultValue=parameter.HasDefaultValue && parameter.DefaultValue is not DBNull && parameter.DefaultValue is not Missing ? parameter.DefaultValue?.ToString():null,
        passing=parameter.IsOut?"out":parameter.ParameterType.IsByRef?"ref":parameter.IsDefined(typeof(ParamArrayAttribute),false)?"params":"value" };
    private static object? Source(Type type,string name,string prefix)
    {
        if(prefix=="P")return SourceDocumentation.Find(prefix,type,name);
        if(SourceDocumentation.ScriptTypes.TryGetProperty(type.FullName!,out var definition))
            return definition.GetProperty("members").EnumerateArray().Where(m=>Text(m,"name")==name).Select(m=>(object)m.Clone()).ToArray();
        return null;
    }
    private static void Remember(Type type)
    {
        if(type.IsByRef||type.IsArray)Remember(type.GetElementType()!);
        if(type.IsGenericType)foreach(var argument in type.GetGenericArguments())Remember(argument);
        if(type.FullName is not null)lock(Gate)KnownTypes.Add(type.FullName);
    }
    private static string Friendly(Type type)=>type.IsGenericType?type.Name.Split('`')[0]+"<"+string.Join(",",type.GetGenericArguments().Select(Friendly))+">":type.FullName??type.Name;
    private static string BindingUsage(JsonElement binding)=>Text(binding,"kind") switch
    {
        "function"=>Text(binding,"alias")+"(...) 是全局函数；以实际签名和 awaitRequired 为准。",
        "type"=>"new "+Text(binding,"alias")+"(...) 或类型静态成员；通过 read 选择真实构造重载。",
        "namespace"=>"OpenCvSharp 程序集导出的完整命名空间与类型，按具体类型分页读取。",
        _=>Text(binding,"alias")+" 是已注入宿主对象；读取其真实方法与属性，不自行 new 包装对象。"
    };
}
