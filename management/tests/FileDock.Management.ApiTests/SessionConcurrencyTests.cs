using FileDock.Management.Data;
using FileDock.Management.Domain.Sessions;
using Microsoft.Extensions.DependencyInjection;

namespace FileDock.Management.ApiTests;

public sealed class SessionConcurrencyTests
{
    [Fact]
    public async Task SignOutBetweenLookupAndTouchDoesNotRaiseConcurrencyException()
    {
        using var fixture = await AuthTests.Fixture.CreateAsync();
        using var scope = fixture.Factory.Services.CreateScope();
        var store = new EfSessionStore(scope.ServiceProvider.GetRequiredService<AppDbContext>());
        var sessions = scope.ServiceProvider.GetRequiredService<SessionService>();
        var issued = await sessions.CreateAsync(fixture.Admin, "127.0.0.1", "race-test");
        await store.DeleteAsync(issued.Session);
        Assert.False(await store.TouchAsync(issued.Session.Id, DateTimeOffset.UtcNow));
        await store.DeleteAsync(issued.Session);
        Assert.Null(await sessions.AuthenticateAsync(issued.Token));
    }
}
