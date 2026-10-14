using FileDock.Management.Data;
using FileDock.Management.Domain;
using FileDock.Management.Domain.Restore;
using Microsoft.EntityFrameworkCore;
namespace FileDock.Management.UnitTests;

public sealed class ManagementConfigurationTests
{
    private static AppDbContext Open(string path) => new(new DbContextOptionsBuilder<AppDbContext>().UseSqlite($"Data Source={path}").AddInterceptors(new ManagementConfigChangeInterceptor()).Options);
    [Fact]
    public async Task ConfigurationCounterIgnoresAuthenticationAndHistory()
    {
        var path = Path.Combine(Path.GetTempPath(), $"fd-config-{Guid.NewGuid():N}.db");
        try
        {
            await using var db = Open(path); await db.Database.MigrateAsync();
            var admin = new Administrator { Id = Guid.NewGuid(), Username = "root", PasswordHash = "hash", CreatedAt = DateTimeOffset.UtcNow, UpdatedAt = DateTimeOffset.UtcNow }; db.Administrators.Add(admin); await db.SaveChangesAsync();
            var settings = await db.Settings.SingleAsync(); var before = settings.ConfigChangeSeq; var changedAt = settings.ConfigChangedAt; Assert.NotNull(changedAt);
            admin.LastSignInAt = DateTimeOffset.UtcNow; admin.FailedAttempts = 1;
            db.Sessions.Add(new Session { Id = Guid.NewGuid(), AdministratorId = admin.Id, TokenHash = new byte[32], CreatedAt = DateTimeOffset.UtcNow, LastSeenAt = DateTimeOffset.UtcNow });
            db.AuditEntries.Add(new AuditEntry { Id = Guid.NewGuid(), At = DateTimeOffset.UtcNow, ActorKind = "System", ActorId = "test", ActorName = "test", Action = "auth", Target = "test", Result = "Succeeded" }); await db.SaveChangesAsync();
            Assert.Equal(before, settings.ConfigChangeSeq); Assert.Equal(changedAt, settings.ConfigChangedAt); admin.PasswordHash = "new-hash"; await db.SaveChangesAsync(); Assert.Equal(before + 1, settings.ConfigChangeSeq);
            var bytes = await new ManagementConfigStore(db).ExportAsync(); var text = System.Text.Encoding.UTF8.GetString(bytes); Assert.DoesNotContain("sessions", text, StringComparison.OrdinalIgnoreCase); Assert.DoesNotContain("audit", text, StringComparison.OrdinalIgnoreCase); Assert.DoesNotContain("lastSignIn", text, StringComparison.OrdinalIgnoreCase); Assert.Contains("new-hash", text, StringComparison.Ordinal);
        }
        finally { File.Delete(path); File.Delete(path + "-wal"); File.Delete(path + "-shm"); }
    }
    [Fact]
    public async Task FreshImportIsAtomicIdempotentAndStartsANewAuditHistory()
    {
        var path = Path.Combine(Path.GetTempPath(), $"fd-import-{Guid.NewGuid():N}.db");
        try
        {
            await using var db = Open(path); await db.Database.MigrateAsync(); var id = Guid.NewGuid();
            var document = System.Text.Json.JsonSerializer.SerializeToUtf8Bytes(new ManagementConfiguration("fdconfig.management/1", 12, [new(id, "root", "hash", true)], [], new(null)), ManagementRestoreService.Json);
            var store = new ManagementConfigStore(db); await store.ImportAsync(document, "operation"); await store.ImportAsync(document, "operation");
            Assert.Single(await db.Administrators.ToListAsync()); Assert.Empty(await db.Sessions.ToListAsync()); Assert.Equal("appliance.restore", (await db.AuditEntries.SingleAsync()).Action); Assert.Equal(12, (await db.Settings.SingleAsync()).ConfigChangeSeq);
            await Assert.ThrowsAsync<InvalidOperationException>(() => store.ImportAsync(document, "another-operation"));
        }
        finally { File.Delete(path); File.Delete(path + "-wal"); File.Delete(path + "-shm"); }
    }
}
