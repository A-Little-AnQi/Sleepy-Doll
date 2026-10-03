using System.Linq;
using System.Text;
using System.Text.RegularExpressions;

namespace BgiBridge.Tools;

/// <summary>为新配置组生成简短自然默认名；用户显式给出的 groupName 不经此处理。</summary>
public static partial class GroupNaming
{
    // 只移除明确的元数据附注：@作者后缀、作者：xxx、vN 版本、日期、时间与长数字戳；
    // 括号只当分隔符剥离，不按括号猜测内容。正文中的“作者”等字面词不动。
    [GeneratedRegex(@"(@\S+)|(作者[:：]\s*\S*)|([vV]\d+(\.\d+)*)|(\d{4}[-/.年]\s*\d{1,2}[-/.月]\s*\d{1,2}日?)|(\d{1,2}:\d{2}(:\d{2})?)|(\d{6,})", RegexOptions.Compiled)]
    private static partial Regex Metadata();

    public static string Default(string source, string fallback)
    {
        var cleaned = Metadata().Replace(source ?? string.Empty, " ");
        var builder = new StringBuilder();
        foreach (var rune in cleaned.EnumerateRunes())
            if (Rune.IsLetterOrDigit(rune))
                builder.Append(rune);
        var name = builder.ToString().Trim();
        if (name.Length == 0) return fallback;
        return name.EnumerateRunes().Count() <= 20 ? name : string.Concat(name.EnumerateRunes().Take(20).Select(r => r.ToString()));
    }
}
