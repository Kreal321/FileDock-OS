using System.Text.Json;
using FileDock.Management.Domain;
using FileDock.Management.Domain.Restore;
using Microsoft.EntityFrameworkCore;
namespace FileDock.Management.Data;

public sealed class ManagementConfigStore(AppDbContext db) : IManagementConfigStore
{
    public async Task<byte[]> ExportAsync(CancellationToken cancellationToken = default)
    {
        await using var transaction = await db.Database.BeginTransactionAsync(cancellationToken);
        var settings = await db.Settings.AsNoTracking().SingleAsync(s => s.Id == 1, cancellationToken);
        var admins = await db.Administrators.AsNoTracking().OrderBy(a => a.Id).ToListAsync(cancellationToken);
        var tokens = await db.ApiTokens.AsNoTracking().OrderBy(t => t.Id).ToListAsync(cancellationToken);
        var config = new ManagementConfiguration("fdconfig.management/1", settings.ConfigChangeSeq,
            admins.Select(a => new AdministratorConfiguration(a.Id, a.Username, a.PasswordHash, a.Enabled)).ToArray(),
            tokens.Select(t => new TokenConfiguration(t.Id, t.Name, t.Scope, t.SecretHash, t.CreatedBy, t.ExpiresAt, t.RevokedAt)).ToArray(), new(settings.ListenAddresses), settings.ConfigChangedAt);
        await transaction.CommitAsync(cancellationToken);
        return JsonSerializer.SerializeToUtf8Bytes(config, ManagementRestoreService.Json);
    }
    public async Task ImportAsync(byte[] document, string operationId, CancellationToken cancellationToken = default)
    {
        var config = ManagementRestoreService.Validate(document);
        await using var transaction = await db.Database.BeginTransactionAsync(cancellationToken);
        var settings = await db.Settings.SingleAsync(s => s.Id == 1, cancellationToken);
        if (settings.LastRestoreOperationId == operationId)
        {
            return;
        }

        if (await db.Administrators.AnyAsync(cancellationToken))
        {
            throw new InvalidOperationException("Restore requires a fresh Management database.");
        }

        db.ImportingConfiguration = true;
        try
        {
            await db.Sessions.ExecuteDeleteAsync(cancellationToken);
            await db.ApiTokens.ExecuteDeleteAsync(cancellationToken);
            await db.AuditEntries.ExecuteDeleteAsync(cancellationToken);
            var now = DateTimeOffset.UtcNow;
            db.Administrators.AddRange(config.Administrators.Select(a => new Administrator { Id = a.Id, Username = a.Username, PasswordHash = a.PasswordHash, Enabled = a.Enabled, CreatedAt = now, UpdatedAt = now }));
            db.ApiTokens.AddRange(config.ApiTokens.Select(t => new ApiToken { Id = t.Id, Name = t.Name, Scope = t.Scope, SecretHash = t.SecretHash, CreatedBy = t.CreatedBy, ExpiresAt = t.ExpiresAt, RevokedAt = t.RevokedAt, CreatedAt = now }));
            settings.ListenAddresses = config.Settings.ListenAddresses; settings.ConfigChangeSeq = config.ConfigSeq; settings.ConfigChangedAt = now; settings.LastRestoreOperationId = operationId;
            var setup = await db.SetupStates.SingleAsync(s => s.Id == 1, cancellationToken);
            setup.Status = "Complete"; setup.SetupCodeHash = null; setup.AttemptCounters = "{}";
            db.AuditEntries.Add(new AuditEntry { Id = Guid.NewGuid(), At = now, ActorKind = "System", ActorId = "restore", ActorName = "restore", Action = "appliance.restore", Target = operationId, Result = "Succeeded" });
            await db.SaveChangesAsync(cancellationToken);
            await transaction.CommitAsync(cancellationToken);
        }
        finally { db.ImportingConfiguration = false; }
    }
}
