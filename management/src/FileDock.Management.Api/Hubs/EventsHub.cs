using System.Collections.Concurrent;
using FileDock.Management.Core;
using FileDock.Management.Data;
using FileDock.Management.Domain;
using FileDock.Management.Domain.Sessions;
using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.SignalR;
using Microsoft.EntityFrameworkCore;

namespace FileDock.Management.Api.Hubs;

[Authorize]
public sealed class EventsHub(EventConnections connections, CoreHealthMonitor health) : Hub
{
    public override async Task OnConnectedAsync()
    {
        var http = Context.GetHttpContext()!;
        var session = http.Items["fd.session"] as Session;
        var token = http.Items["fd.token"] as ApiToken;
        if (session is null && token is null) { Context.Abort(); return; }
        connections.Add(Context.ConnectionId, new(session?.Id, session?.AdministratorId, token?.Id, Context.Abort));
        await Clients.Caller.SendAsync("core-health", health.Current, Context.ConnectionAborted);
        // A browser reconnect always reloads its views, including changes received while disconnected.
        await Clients.Caller.SendAsync("sync", Context.ConnectionAborted);
        await base.OnConnectedAsync();
    }
    public override Task OnDisconnectedAsync(Exception? exception)
    {
        connections.Remove(Context.ConnectionId);
        return base.OnDisconnectedAsync(exception);
    }
}

public sealed record EventConnection(Guid? SessionId, Guid? AdministratorId, Guid? TokenId, Action Abort);
public sealed class EventConnections(IServiceScopeFactory scopes, IHubContext<EventsHub> hub, ILogger<EventConnections> logger) : BackgroundService
{
    private static readonly Action<ILogger, string, Exception?> _validationFailed = LoggerMessage.Define<string>(LogLevel.Warning, new EventId(103, "EventValidationFailed"), "Event connection validation failed ({ErrorType})");
    private readonly ConcurrentDictionary<string, EventConnection> _connections = new();
    public void Add(string id, EventConnection connection) => _connections[id] = connection;
    public void Remove(string id) => _connections.TryRemove(id, out _);
    private static async Task<bool> ValidAsync(AppDbContext db, EventConnection connection, CancellationToken ct)
    {
        if (connection.TokenId is Guid tokenId)
        {
            var token = await db.ApiTokens.AsNoTracking().SingleOrDefaultAsync(t => t.Id == tokenId, ct);
            return token is not null && token.RevokedAt is null && (token.ExpiresAt is null || token.ExpiresAt > DateTimeOffset.UtcNow);
        }
        var session = await db.Sessions.AsNoTracking().SingleOrDefaultAsync(s => s.Id == connection.SessionId, ct);
        return session is not null && DateTimeOffset.UtcNow - session.LastSeenAt <= SessionService.IdleExpiry
            && await db.Administrators.AnyAsync(a => a.Id == connection.AdministratorId && a.Enabled, ct);
    }
    public async Task BroadcastAsync(string method, object value, CancellationToken ct)
    {
        using var scope = scopes.CreateScope();
        var db = scope.ServiceProvider.GetRequiredService<AppDbContext>();
        foreach (var (id, connection) in _connections)
        {
            if (await ValidAsync(db, connection, ct)) { await hub.Clients.Client(id).SendAsync(method, value, ct); }
            else { await CloseAsync(id, connection, ct); }
        }
    }
    private async Task CloseAsync(string id, EventConnection connection, CancellationToken ct)
    {
        Remove(id);
        try { await hub.Clients.Client(id).SendAsync("session-expired", ct); }
        finally { connection.Abort(); }
    }
    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        while (!stoppingToken.IsCancellationRequested)
        {
            try
            {
                using var scope = scopes.CreateScope();
                var db = scope.ServiceProvider.GetRequiredService<AppDbContext>();
                foreach (var (id, connection) in _connections)
                {
                    if (!await ValidAsync(db, connection, stoppingToken)) { await CloseAsync(id, connection, stoppingToken); }
                }
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested) { break; }
            catch (Exception error) { _validationFailed(logger, error.GetType().Name, null); }
            await Task.Delay(TimeSpan.FromSeconds(1), stoppingToken);
        }
    }
}

public sealed class ManagementEventSink(IServiceScopeFactory scopes, EventConnections connections) : IManagementEventSink
{
    public async Task<ulong> ReadCursorAsync(CancellationToken cancellationToken)
    {
        using var scope = scopes.CreateScope();
        var db = scope.ServiceProvider.GetRequiredService<AppDbContext>();
        return (ulong)(await db.Settings.AsNoTracking().SingleAsync(cancellationToken)).LastEventSeq;
    }
    public Task PublishAsync(CoreEventDto change, CancellationToken cancellationToken) => connections.BroadcastAsync("event", change, cancellationToken);
    public Task PublishHealthAsync(CoreHealth health, CancellationToken cancellationToken) => connections.BroadcastAsync("core-health", health, cancellationToken);
    public async Task SaveCursorAsync(ulong seq, CancellationToken cancellationToken)
    {
        using var scope = scopes.CreateScope();
        var db = scope.ServiceProvider.GetRequiredService<AppDbContext>();
        await db.Settings.ExecuteUpdateAsync(s => s.SetProperty(x => x.LastEventSeq, (long)seq), cancellationToken);
    }
}
