using System.Collections;
using System.Text.Json;
using BgiBridge.Tools;

namespace BgiBridge.Bgi;

/// <summary>业务观察在适配层解释真实宿主证据，通用命令计划不推测处理器的业务结果。</summary>
internal sealed class NativeCommandEvidence
{
    private readonly HostLogTools.Cursor? cursor;
    private readonly string[] codes;
    private NativeCommandEvidence(string[] codes,HostLogTools.Cursor? cursor){this.codes=codes;this.cursor=cursor;}
    internal static NativeCommandEvidence Begin(string command,object? argument)
    {
        if(command!="feed_window.auto_redeem_item"||argument is null)return new([],null);
        var values=argument.GetType().GetProperty("Codes")?.GetValue(argument) as IEnumerable;
        var codes=values?.Cast<object>().OfType<string>().Select(code=>code.Trim()).Where(code=>code.Length>0).Distinct(StringComparer.Ordinal).Take(33).ToArray()??[];
        if(codes.Length==0||codes.Length>32||codes.Any(code=>code.Length>128||code.Any(char.IsControl)))return new([],null);
        return new(codes,HostLogTools.CaptureCursor());
    }
    internal async Task<JsonElement?> Read(CancellationToken cancellation)
    {
        if(codes.Length==0)return null;
        for(var attempt=0;attempt<8;attempt++) {
            cancellation.ThrowIfCancellationRequested();
            var evidence=Classify(codes,HostLogTools.ReadSince(cursor)??[],Environment.ProcessId);
            if(evidence.GetProperty("status").GetString()!="unknown")return evidence;
            if(attempt<7)await Task.Delay(100,cancellation);
        }
        return JsonSerializer.SerializeToElement(new{status="unknown",scope="redeemCodeLog",reason="本次命令已返回，但未取得带当前进程标识的完整逐码业务回执；没有把已启动、已使用或未知结果当成功。"});
    }
    internal static JsonElement Classify(IEnumerable<string> expected,IEnumerable<HostLogTools.HostLogEntry> entries,int processId)
    {
        var relevant=entries.Where(entry=>entry.Logger.EndsWith("UseRedemptionCodeTask",StringComparison.Ordinal)
            && System.Text.RegularExpressions.Regex.IsMatch(entry.Thread,$@"(?:^|:)P{processId}(?=:|$)",System.Text.RegularExpressions.RegexOptions.CultureInvariant,TimeSpan.FromSeconds(1))).ToArray();
        var results=expected.Select(code=>{
            var prefix=$"兑换码 {code} ";
            var failure=relevant.Any(entry=>entry.Body.Contains(prefix+"兑换失败",StringComparison.Ordinal));
            var success=relevant.Any(entry=>entry.Body.Contains(prefix+"兑换成功",StringComparison.Ordinal));
            return new{code,status=failure?"failed":success?"succeeded":"unknown"};
        }).ToArray();
        var status=results.Any(result=>result.status=="failed")?"failed":results.Length>0&&results.All(result=>result.status=="succeeded")?"succeeded":"unknown";
        return JsonSerializer.SerializeToElement(new{status,scope="redeemCodeLog",processId,codes=results,reason=status=="failed"?"宿主本次逐码日志报告兑换失败；日志未区分过期、错误或已使用，不猜原因。":status=="succeeded"?"宿主本次逐码日志全部明确报告兑换成功。":"本次逐码回执不完整。"});
    }
}
