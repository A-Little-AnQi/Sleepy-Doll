using System.IO.Compression;
using System.Reflection.Metadata;
using System.Reflection.PortableExecutable;
using System.Text.Json;

namespace BgiBridge.Bgi;

/// <summary>只读程序集及内嵌 PDB；不联网、不执行目标代码、不按版本或文件指纹列白名单。</summary>
public static class HostOrigin
{
    public const string RejectedMessage = "连接失败，请使用官方版本的BetterGI。";
    private static readonly Guid SourceLinkKind = new("CC110556-A091-4D38-9FEC-25AB9A351A6A");
    private static readonly byte[] BundleSignature = Convert.FromHexString("8b1202b96a612038727b930214d7a03213f5b9e6efae3318ee3b2dce24b36aae");
    private const long MaxAssemblyBytes = 128 * 1024 * 1024;
    public sealed record Assessment(string State, string Evidence, string? Repository = null);
    private static readonly Lazy<Assessment> CurrentAssessment = new(() => Inspect(Environment.ProcessPath ?? ""));
    public static Assessment Current => CurrentAssessment.Value;
    public static void RequireOfficial()
    {
        if (Current.State != "official") throw new BgiBridge.Protocol.BridgeException("HOST_ORIGIN_REJECTED", RejectedMessage, 403);
    }

    public static Assessment Inspect(string path)
    {
        try
        {
            using var input = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            using var image = ReadAssembly(input);
            using var pe = new PEReader(image);
            if (!pe.HasMetadata) return new("unknown", "managedMetadataMissing");
            var metadata = pe.GetMetadataReader();
            if (!metadata.IsAssembly || metadata.GetString(metadata.GetAssemblyDefinition().Name) != "BetterGI")
                return new("unknown", "hostAssemblyMismatch");
            foreach (var entry in pe.ReadDebugDirectory())
            {
                if (entry.Type != DebugDirectoryEntryType.EmbeddedPortablePdb) continue;
                if (entry.DataPointer < 0 || entry.DataSize < 8 || entry.DataSize > 32 * 1024 * 1024 || entry.DataPointer > image.Length - entry.DataSize)
                    return new("unknown", "pdbRangeInvalid");
                image.Position = entry.DataPointer + 4;
                using (var header = new BinaryReader(image, System.Text.Encoding.UTF8, leaveOpen: true))
                    if (header.ReadUInt32() > 32 * 1024 * 1024) return new("unknown", "pdbSizeInvalid");
                using var provider = pe.ReadEmbeddedPortablePdbDebugDirectoryData(entry);
                var pdb = provider.GetMetadataReader();
                foreach (var handle in pdb.CustomDebugInformation)
                {
                    var item = pdb.GetCustomDebugInformation(handle);
                    if (pdb.GetGuid(item.Kind) != SourceLinkKind) continue;
                    var documents = pdb.Documents.Select(handle => pdb.GetString(pdb.GetDocument(handle).Name)).ToArray();
                    return InspectSourceLink(pdb.GetBlobBytes(item.Value), documents);
                }
            }
            return new("unknown", "sourceLinkMissing");
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException or BadImageFormatException or JsonException or InvalidDataException or ArgumentException or OverflowException)
        {
            return new("unknown", "originUnreadable");
        }
    }

    public static Assessment InspectSourceLink(ReadOnlySpan<byte> content, string[]? sourceDocuments = null)
    {
        using var document = JsonDocument.Parse(content.ToArray());
        if (!document.RootElement.TryGetProperty("documents", out var documents) || documents.ValueKind != JsonValueKind.Object)
            return new("unknown", "sourceLinkDocumentsMissing");
        var repositories = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var item in documents.EnumerateObject())
        {
            if (sourceDocuments is not null && !sourceDocuments.Any(path => Covers(item.Name, path))) continue;
            if (item.Value.ValueKind != JsonValueKind.String || !Uri.TryCreate(item.Value.GetString(), UriKind.Absolute, out var uri))
                return new("unknown", "sourceLinkUrlInvalid");
            if (uri.Scheme != "https" || uri.UserInfo.Length != 0 || !uri.IsDefaultPort || !uri.Host.Equals("raw.githubusercontent.com", StringComparison.OrdinalIgnoreCase))
                return new("nonOfficial", "sourceLinkHost", uri.Host);
            var segments = uri.AbsolutePath.Split('/', StringSplitOptions.RemoveEmptyEntries);
            if (segments.Length < 4) return new("unknown", "sourceLinkPathInvalid");
            repositories.Add($"{segments[0]}/{segments[1]}");
        }
        if (repositories.Count == 0) return new("unknown", "sourceLinkEmpty");
        // 官方身份是固定的策略锚点；分支、提交、版本和正式／测试渠道均不参与比较。
        var foreign = repositories.FirstOrDefault(repository => !repository.Equals("babalae/better-genshin-impact", StringComparison.OrdinalIgnoreCase));
        return foreign is null ? new("official", "embeddedSourceLink", "babalae/better-genshin-impact") : new("nonOfficial", "embeddedSourceLink", foreign);
    }

    private static bool Covers(string pattern, string path)
    {
        pattern = pattern.Replace('\\', '/'); path = path.Replace('\\', '/');
        var wildcard = pattern.IndexOf('*');
        return wildcard < 0 ? path == pattern : path.StartsWith(pattern[..wildcard], StringComparison.Ordinal) && path.EndsWith(pattern[(wildcard + 1)..], StringComparison.Ordinal);
    }

    private static Stream ReadAssembly(FileStream input)
    {
        using (var candidate = new PEReader(input, PEStreamOptions.LeaveOpen))
            if (candidate.HasMetadata) { input.Position = 0; return ReadBounded(input, input.Length); }
        input.Position = 0;
        var prefix = new byte[(int)Math.Min(input.Length, 4 * 1024 * 1024)];
        input.ReadExactly(prefix);
        var marker = prefix.AsSpan().IndexOf(BundleSignature);
        if (marker < 8) throw new InvalidDataException("Bundle marker missing");
        var header = System.Buffers.Binary.BinaryPrimitives.ReadInt64LittleEndian(prefix.AsSpan(marker - 8, 8));
        if (header < 0 || header >= input.Length) throw new InvalidDataException("Bundle header outside file");
        input.Position = header;
        using var reader = new BinaryReader(input, System.Text.Encoding.UTF8, leaveOpen: true);
        var major = reader.ReadUInt32(); reader.ReadUInt32();
        var count = reader.ReadInt32(); ReadBundleString(reader);
        if (count < 1 || count > 20000) throw new InvalidDataException("Bundle entry count invalid");
        if (major >= 2) for (var field = 0; field < 5; field++) reader.ReadInt64();
        for (var index = 0; index < count; index++)
        {
            var offset = reader.ReadInt64(); var size = reader.ReadInt64();
            var compressed = major >= 6 ? reader.ReadInt64() : 0;
            reader.ReadByte(); var name = ReadBundleString(reader);
            if (name != "BetterGI.dll") continue;
            var stored = compressed > 0 ? compressed : size;
            if (offset < 0 || size < 1 || stored < 1 || offset > input.Length - stored || size > MaxAssemblyBytes || stored > MaxAssemblyBytes)
                throw new InvalidDataException("Bundle assembly range invalid");
            input.Position = offset;
            if (compressed == 0) return ReadBounded(input, size);
            using var packed = ReadBounded(input, compressed);
            using var inflater = new DeflateStream(packed, CompressionMode.Decompress);
            var result = ReadBounded(inflater, size);
            if (inflater.ReadByte() != -1) { result.Dispose(); throw new InvalidDataException("Bundle assembly exceeds declared size"); }
            return result;
        }
        throw new InvalidDataException("Host assembly missing from bundle");
    }

    private static MemoryStream ReadBounded(Stream source, long size)
    {
        if (size < 1 || size > MaxAssemblyBytes) throw new InvalidDataException("Assembly size invalid");
        var bytes = new byte[(int)size]; source.ReadExactly(bytes); return new MemoryStream(bytes, writable: false);
    }

    private static string ReadBundleString(BinaryReader reader)
    {
        var size = reader.Read7BitEncodedInt();
        if (size < 0 || size > 4096) throw new InvalidDataException("Bundle string size invalid");
        var bytes = reader.ReadBytes(size);
        if (bytes.Length != size) throw new EndOfStreamException();
        return System.Text.Encoding.UTF8.GetString(bytes);
    }
}
