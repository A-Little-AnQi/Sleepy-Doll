using System.Collections.Concurrent;
using System.Text.Json;

namespace BgiBridge.Jobs;

/// <summary>Job 生命周期，对齐 <c>docs/bgi-implementation-plan.md</c> §5.4。</summary>
public static class JobState
{
    public const string Queued = "queued";
    public const string Running = "running";
    public const string Cancelling = "cancelling";
    public const string Cancelled = "cancelled";
    public const string Completed = "completed";
    public const string Failed = "failed";
    public const string Interrupted = "interrupted";

    /// <summary>仍可能持有执行权，不能当已取消处理。</summary>
    public const string StoppingUnconfirmed = "stoppingUnconfirmed";

    public static bool IsTerminal(string state) =>
        state is Cancelled or Completed or Failed or Interrupted;
}

/// <summary>BGI 不提供权威业务结果，默认 unknown，由 agent 用 state 观测收敛。</summary>
public static class Verification
{
    public const string Unknown = "unknown";
}

public sealed record JobSnapshot(
    string JobId,
    string MethodId,
    string State,
    object? Result,
    string VerificationStatus,
    string? VerificationReason,
    string? Error,
    DateTimeOffset AcceptedAt,
    DateTimeOffset? EndedAt)
{
    public object ToWire() => new
    {
        jobId = JobId,
        methodId = MethodId,
        state = State,
        result = Result,
        verification = new { status = VerificationStatus, reason = VerificationReason },
        error = Error,
        timestamps = new { acceptedAt = AcceptedAt, endedAt = EndedAt },
    };
}

/// <summary>
/// 内存 Job 表。已知精度损失：mcp 分支能在 TaskRunner 里精确感知「本次启动真的拿到锁」，
/// 那需要改源码；这里只能轮询 TaskSemaphore 近似，不区分 running 是否已确认。
/// </summary>
public sealed class JobStore(int keep = 200)
{
    private readonly ConcurrentDictionary<string, JobSnapshot> _jobs = new(StringComparer.Ordinal);

    public JobSnapshot Create(string methodId)
    {
        var id = Guid.NewGuid().ToString("N");
        var snapshot = new JobSnapshot(
            id, methodId, JobState.Queued, null, Verification.Unknown,
            "The method completed without an authoritative goal result", null,
            DateTimeOffset.UtcNow, null);
        _jobs[id] = snapshot;
        Prune();
        return snapshot;
    }

    public JobSnapshot? Get(string id) => _jobs.TryGetValue(id, out var job) ? job : null;

    public JobSnapshot Update(string id, Func<JobSnapshot, JobSnapshot> change)
    {
        var updated = _jobs.AddOrUpdate(id, _ => throw new KeyNotFoundException(id), (_, current) => change(current));
        return updated;
    }

    public JobSnapshot MarkRunning(string id) =>
        Update(id, job => job with { State = JobState.Running });

    public JobSnapshot MarkCompleted(string id, object? result, bool verified = false) =>
        Update(id, job => job with
        {
            State = JobState.Completed,
            Result = result,
            VerificationStatus = verified ? "succeeded" : Verification.Unknown,
            VerificationReason = verified ? "配置内存值与原子落盘结果已核验。" : "处理器返回不代表业务目标已验证。",
            EndedAt = DateTimeOffset.UtcNow,
        });

    public JobSnapshot MarkFailed(string id, string error) =>
        Update(id, job => job with
        {
            State = JobState.Failed,
            Error = error,
            EndedAt = DateTimeOffset.UtcNow,
        });

    public JobSnapshot MarkCancelling(string id) =>
        Update(id, job => job with { State = JobState.Cancelling });

    public JobSnapshot MarkCancelled(string id) =>
        Update(id, job => job with
        {
            State = JobState.Cancelled,
            EndedAt = DateTimeOffset.UtcNow,
        });

    public IEnumerable<JobSnapshot> All => _jobs.Values.OrderByDescending(x => x.AcceptedAt);

    private void Prune()
    {
        foreach (var stale in _jobs.Values.Where(job => JobState.IsTerminal(job.State)).OrderByDescending(x => x.AcceptedAt).Skip(keep))
            _jobs.TryRemove(stale.JobId, out _);
    }
}
