using FileDock.Management.Domain;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Microsoft.EntityFrameworkCore.Storage;
namespace FileDock.Management.Data;
/// Configuration-only changes increment the counter in the same SaveChanges transaction.
public sealed class ManagementConfigChangeInterceptor : SaveChangesInterceptor
{
    private static void Capture(DbContext? context)
    {
        if (context is not AppDbContext db || db.ImportingConfiguration)
        {
            return;
        }

        db.ChangeTracker.DetectChanges();
        var changed = db.ChangeTracker.Entries().Any(e => e.Entity switch
        {
            Administrator => e.State is EntityState.Added or EntityState.Deleted || e.Properties.Any(p => p.IsModified && p.Metadata.Name is "Username" or "PasswordHash" or "Enabled"),
            ApiToken => e.State is EntityState.Added or EntityState.Deleted || e.Properties.Any(p => p.IsModified && p.Metadata.Name is "Name" or "Scope" or "SecretHash" or "CreatedBy" or "ExpiresAt" or "RevokedAt"),
            Settings => e.Properties.Any(p => p.IsModified && p.Metadata.Name == "ListenAddresses"),
            _ => false,
        });
        if (!changed)
        {
            return;
        }

        var settings = db.Settings.Local.FirstOrDefault(s => s.Id == 1) ?? db.Settings.Single(s => s.Id == 1);
        using var command = db.Database.GetDbConnection().CreateCommand();
        command.Transaction = db.Database.CurrentTransaction?.GetDbTransaction() ?? throw new InvalidOperationException("Configuration changes require a transaction.");
        command.CommandText = "UPDATE Settings SET ConfigChangeSeq = ConfigChangeSeq + 1 WHERE Id = 1 RETURNING ConfigChangeSeq";
        var seq = Convert.ToInt64(command.ExecuteScalar(), System.Globalization.CultureInfo.InvariantCulture);
        settings.ConfigChangedAt = DateTimeOffset.UtcNow;
        var property = db.Entry(settings).Property(s => s.ConfigChangeSeq); property.OriginalValue = seq; property.CurrentValue = seq; property.IsModified = false;
    }
    public override InterceptionResult<int> SavingChanges(DbContextEventData eventData, InterceptionResult<int> result) { Capture(eventData.Context); return result; }
    public override ValueTask<InterceptionResult<int>> SavingChangesAsync(DbContextEventData eventData, InterceptionResult<int> result, CancellationToken cancellationToken = default) { Capture(eventData.Context); return ValueTask.FromResult(result); }
}
