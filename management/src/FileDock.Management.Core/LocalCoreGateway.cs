using System.Collections.Concurrent;
using System.Security.Cryptography;

namespace FileDock.Management.Core;

/// <summary>In-process Core stand-in for tests and hosts without core.sock.</summary>
public sealed class LocalCoreGateway : ICoreGateway
{
    private const string _alphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    private readonly ConcurrentDictionary<string, bool> _recoveryKeys = new();
    private string _applianceName = "";

    public Task ApplyApplianceNameAsync(string applianceName, string actor, string requestId, CancellationToken cancellationToken = default)
    {
        _applianceName = applianceName;
        return Task.CompletedTask;
    }

    public Task<GeneratedRecoveryKeyDto> GenerateRecoveryKeyAsync(string actor, string requestId, CancellationToken cancellationToken = default)
    {
        var raw = new char[25];
        var bytes = RandomNumberGenerator.GetBytes(25);
        for (var i = 0; i < raw.Length; i++)
        {
            raw[i] = _alphabet[bytes[i] % _alphabet.Length];
        }

        var groups = Enumerable.Range(0, 5).Select(i => new string(raw, i * 5, 5)).ToArray();
        var formatted = string.Join('-', groups);
        var keyId = Convert.ToHexString(SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(formatted)))[..16].ToLowerInvariant();
        _recoveryKeys[keyId] = false;
        return Task.FromResult(new GeneratedRecoveryKeyDto(keyId, formatted));
    }

    public Task ConfirmRecoveryKeyAsync(string keyId, string actor, string requestId, CancellationToken cancellationToken = default)
    {
        if (!_recoveryKeys.ContainsKey(keyId))
        {
            throw new InvalidOperationException("Unknown recovery key.");
        }

        _recoveryKeys[keyId] = true;
        return Task.CompletedTask;
    }

    public Task<SystemStatusDto> GetSystemStatusAsync(CancellationToken cancellationToken = default)
    {
        var components = new[]
        {
            new ComponentHealthDto("core", "FileDock Core", "Healthy", "local"),
            new ComponentHealthDto("management", "Management Service", "Healthy", "serving"),
            new ComponentHealthDto("docker", "Docker Engine", "Healthy", "local"),
            new ComponentHealthDto("samba", "Samba", "Healthy", "local"),
            new ComponentHealthDto("nfs", "NFS", "Healthy", "local"),
            new ComponentHealthDto("kopia", "Kopia", "Healthy", "local"),
            new ComponentHealthDto("rclone", "rclone", "Healthy", "local"),
            new ComponentHealthDto("fd-s3", "Native S3 connector", "Healthy", "local"),
        };
        return Task.FromResult(new SystemStatusDto("1.0", "0.1.0", _applianceName, components, false, true));
    }

}
