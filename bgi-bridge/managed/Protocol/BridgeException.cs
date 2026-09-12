namespace BgiBridge.Protocol;

/// <summary>桥的错误类型，code 对齐 docs/bgi-implementation-plan.md §5.5。</summary>
public sealed class BridgeException(string code, string message, int httpStatus = 400)
    : Exception(message)
{
    public string Code { get; } = code;
    public int HttpStatus { get; } = httpStatus;

    public static BridgeException NotFound(string what) =>
        new("METHOD_NOT_FOUND", what, 404);

    public static BridgeException InvalidArgument(string message) =>
        new("INVALID_ARGUMENT", message);

    public static BridgeException Unauthorized(string message = "缺少或错误的 Bearer token。") =>
        new("UNAUTHORIZED", message, 401);

    public static BridgeException Disabled(string message) =>
        new("DISABLED", message, 503);

    public static BridgeException Busy(string message) =>
        new("BUSY", message, 409);

    public static BridgeException GameNotReady(string message) =>
        new("GAME_NOT_READY", message, 409);

    /// <summary>宿主里找不到该类型或成员，通常是版本不匹配。</summary>
    public static BridgeException Missing(string message) =>
        new("HOST_CAPABILITY_MISSING", message, 501);

    public static BridgeException Failed(string message) =>
        new("EXECUTION_FAILED", message, 500);
}
