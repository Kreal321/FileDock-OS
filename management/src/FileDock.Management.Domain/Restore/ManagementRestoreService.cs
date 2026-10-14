using System.Text.Json;
namespace FileDock.Management.Domain.Restore;

public interface IManagementConfigStore
{
    Task<byte[]> ExportAsync(CancellationToken cancellationToken = default);
    Task ImportAsync(byte[] document, string operationId, CancellationToken cancellationToken = default);
}
public sealed record ManagementConfiguration(string Schema, long ConfigSeq, AdministratorConfiguration[] Administrators, TokenConfiguration[] ApiTokens, ManagementSettings Settings, DateTimeOffset? ConfigChangedAt = null);
public sealed record AdministratorConfiguration(Guid Id, string Username, string PasswordHash, bool Enabled);
public sealed record TokenConfiguration(Guid Id, string Name, string Scope, byte[] SecretHash, Guid CreatedBy, DateTimeOffset? ExpiresAt, DateTimeOffset? RevokedAt);
public sealed record ManagementSettings(string? ListenAddresses);
public static class ManagementRestoreService
{
    public static readonly JsonSerializerOptions Json = new(JsonSerializerDefaults.Web);
    public static ManagementConfiguration Validate(byte[] document)
    {
        if (document.Length > 4 * 1024 * 1024) { throw new ArgumentException("Management configuration exceeds the limit."); }
        var config = JsonSerializer.Deserialize<ManagementConfiguration>(document, Json) ?? throw new ArgumentException("Management configuration is missing.");
        if (config.Schema != "fdconfig.management/1" || config.ConfigSeq < 0 || config.Administrators is null || config.ApiTokens is null || config.Settings is null
            || config.Administrators.Length is < 1 or > 1000 || config.ApiTokens.Length > 10000
            || !config.Administrators.Any(a => a.Enabled) || config.Administrators.Select(a => a.Id).Distinct().Count() != config.Administrators.Length
            || config.Administrators.Select(a => a.Username).Distinct(StringComparer.OrdinalIgnoreCase).Count() != config.Administrators.Length
            || config.Administrators.Any(a => a.Id == Guid.Empty || a.Username.Length is < 1 or > 64 || string.IsNullOrEmpty(a.PasswordHash) || a.PasswordHash.Length > 4096)
            || config.ApiTokens.Select(t => t.Id).Distinct().Count() != config.ApiTokens.Length
            || config.ApiTokens.Any(t => t.Id == Guid.Empty || t.Name.Length is < 1 or > 128 || t.SecretHash.Length != 32 || t.Scope is not ("ReadOnly" or "Full") || t.CreatedBy == Guid.Empty))
        { throw new ArgumentException("Management configuration is invalid."); }
        if (config.Settings.ListenAddresses is { } addresses)
        {
            var values = JsonSerializer.Deserialize<string[]>(addresses) ?? throw new ArgumentException("Invalid listener settings.");
            if (values.Length is < 1 or > 64 || values.Any(a => !System.Net.IPAddress.TryParse(a, out _))) { throw new ArgumentException("Invalid listener settings."); }
        }
        return config;
    }
}
