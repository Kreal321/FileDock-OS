using FileDock.Management.Domain;
using FileDock.Management.Domain.Audit;
using Microsoft.EntityFrameworkCore;

namespace FileDock.Management.Data;

public sealed class AppDbContext(DbContextOptions<AppDbContext> options) : DbContext(options)
{
    public DbSet<Administrator> Administrators => Set<Administrator>();
    public DbSet<Session> Sessions => Set<Session>();
    public DbSet<ApiToken> ApiTokens => Set<ApiToken>();
    public DbSet<AuditEntry> AuditEntries => Set<AuditEntry>();
    public DbSet<SetupState> SetupStates => Set<SetupState>();
    public DbSet<Settings> Settings => Set<Settings>();

    protected override void OnModelCreating(ModelBuilder modelBuilder)
    {
        modelBuilder.Entity<Administrator>(e =>
        {
            e.HasIndex(x => x.Username).IsUnique();
            e.Property(x => x.Username).HasMaxLength(64);
        });
        modelBuilder.Entity<ApiToken>(e => e.HasIndex(x => x.Name).IsUnique());
        modelBuilder.Entity<SetupState>().HasData(new SetupState { Id = 1, Status = "AwaitingSetup" });
        modelBuilder.Entity<Settings>().HasData(new Settings { Id = 1 });
        modelBuilder.Entity<AuditEntry>().HasIndex(x => x.At);
    }
}

public sealed class EfAuditStore(AppDbContext db) : IAuditStore
{
    public async Task AddAsync(AuditEntry entry, CancellationToken cancellationToken = default)
    {
        db.AuditEntries.Add(entry);
        await db.SaveChangesAsync(cancellationToken);
    }

    public async Task DeleteOlderThanAsync(DateTimeOffset cutoff, CancellationToken cancellationToken = default)
    {
        var entries = await db.AuditEntries.AsNoTracking().ToListAsync(cancellationToken);
        db.AuditEntries.RemoveRange(entries.Where(e => e.At < cutoff));
        await db.SaveChangesAsync(cancellationToken);
    }
}
