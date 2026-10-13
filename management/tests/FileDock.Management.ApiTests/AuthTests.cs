using System.Net;
using System.Net.Http.Json;
using System.Security.Claims;
using System.Text.Json;
using FileDock.Management.Api.Auth;
using FileDock.Management.Data;
using FileDock.Management.Domain;
using FileDock.Management.Domain.Admins;
using FileDock.Management.Domain.Audit;
using FileDock.Management.Domain.Sessions;
using FileDock.Management.Domain.Setup;
using FileDock.Management.Domain.Tokens;
using Microsoft.AspNetCore.Http;
using Microsoft.AspNetCore.Mvc.Testing;
using Microsoft.Extensions.DependencyInjection;

namespace FileDock.Management.ApiTests;

public sealed class AuthTests
{
    private static readonly string[] TokenScopes = ["Full", "ReadOnly"];
    private static readonly string[] InvalidAddresses = ["https://localhost"];
    private static readonly string[] IPv6Loopback = ["::1"];
    [Fact]
    public async Task AnonymousAccessAndCsrfFailuresAreAudited()
    {
        using var fixture = await Fixture.CreateAsync();
        Assert.Equal(HttpStatusCode.Unauthorized, (await fixture.Client.GetAsync("/api/v1/system/status")).StatusCode);
        fixture.UseSession(csrf: false);
        Assert.Equal(HttpStatusCode.Forbidden, (await fixture.Client.PostAsync("/api/v1/auth/sign-out", null)).StatusCode);
        using var scope = fixture.Factory.Services.CreateScope();
        var entries = scope.ServiceProvider.GetRequiredService<AppDbContext>().AuditEntries.ToList();
        Assert.Contains(entries, e => e.Action == "auth.denied" && e.Result == "Refused");
        Assert.Contains(entries, e => e.Action == "auth.csrf" && e.Result == "Refused");
    }

    [Fact]
    public async Task FiveFailuresLockAccountAndIdleSessionExpires()
    {
        using var fixture = await Fixture.CreateAsync();
        for (var i = 0; i < 4; i++)
        {
            Assert.Equal(HttpStatusCode.Unauthorized, (await fixture.SignIn("wrong")).StatusCode);
        }

        Assert.Equal(HttpStatusCode.TooManyRequests, (await fixture.SignIn("wrong")).StatusCode);
        Assert.Equal(HttpStatusCode.TooManyRequests, (await fixture.SignIn("password")).StatusCode);
        fixture.UseSession();
        using (var scope = fixture.Factory.Services.CreateScope())
        {
            var db = scope.ServiceProvider.GetRequiredService<AppDbContext>();
            db.Sessions.Single().LastSeenAt = DateTimeOffset.UtcNow.AddMinutes(-31);
            await db.SaveChangesAsync();
        }
        Assert.Equal(HttpStatusCode.Unauthorized, (await fixture.Client.GetAsync("/api/v1/system/status")).StatusCode);
    }

    [Fact]
    public async Task TokensAreScopedHashedSampledAndImmediatelyRevocable()
    {
        using var fixture = await Fixture.CreateAsync();
        var (id, token) = await fixture.CreateToken("polling", "ReadOnly");
        fixture.UseToken(token);
        Assert.Equal(HttpStatusCode.OK, (await fixture.Client.GetAsync("/api/v1/system/status")).StatusCode);
        Assert.Equal(HttpStatusCode.OK, (await fixture.Client.GetAsync("/api/v1/system/status")).StatusCode);
        Assert.Equal(HttpStatusCode.Forbidden, (await fixture.Client.PostAsJsonAsync("/api/v1/folders", new { })).StatusCode);
        Assert.Equal(HttpStatusCode.Forbidden, (await fixture.Client.GetAsync("/api/v1/tokens")).StatusCode);
        using (var scope = fixture.Factory.Services.CreateScope())
        {
            var db = scope.ServiceProvider.GetRequiredService<AppDbContext>();
            Assert.StartsWith("argon2id$", System.Text.Encoding.UTF8.GetString(db.ApiTokens.Single().SecretHash));
            Assert.Single(db.AuditEntries.ToList(), e => e.Action == "token.use" && e.Detail.Contains("GET", StringComparison.Ordinal));
            Assert.Contains(db.AuditEntries.ToList(), e => e.ActorName == "polling" && e.Action == "auth.denied");
        }
        fixture.UseSession();
        (await fixture.Client.DeleteAsync($"/api/v1/tokens/{id}")).EnsureSuccessStatusCode();
        fixture.UseToken(token);
        Assert.Equal(HttpStatusCode.Unauthorized, (await fixture.Client.GetAsync("/api/v1/system/status")).StatusCode);
    }

    [Fact]
    public async Task FullTokensCannotManageAccountsTokensOrSetupAndExpiredTokensFail()
    {
        using var fixture = await Fixture.CreateAsync();
        var (id, token) = await fixture.CreateToken("automation", "Full");
        fixture.UseToken(token);
        Assert.Equal(HttpStatusCode.OK, (await fixture.Client.GetAsync("/api/v1/system/status")).StatusCode);
        Assert.Equal(HttpStatusCode.NotFound, (await fixture.Client.PostAsJsonAsync("/api/v1/folders", new { })).StatusCode);
        Assert.Equal(HttpStatusCode.Forbidden, (await fixture.Client.PostAsJsonAsync("/api/v1/tokens", new { name = "another", scope = "Full" })).StatusCode);
        Assert.Equal(HttpStatusCode.Forbidden, (await fixture.Client.GetAsync("/api/v1/admins")).StatusCode);
        Assert.Equal(HttpStatusCode.Forbidden, (await fixture.Client.PostAsJsonAsync("/api/v1/setup/recovery-key/confirm", new { keyId = "x" })).StatusCode);
        using (var scope = fixture.Factory.Services.CreateScope())
        {
            var db = scope.ServiceProvider.GetRequiredService<AppDbContext>();
            (await db.ApiTokens.FindAsync(id))!.ExpiresAt = DateTimeOffset.UtcNow.AddSeconds(-1);
            await db.SaveChangesAsync();
        }
        Assert.Equal(HttpStatusCode.Unauthorized, (await fixture.Client.GetAsync("/api/v1/system/status")).StatusCode);
        fixture.UseToken("fdt_invalid_invalid");
        Assert.Equal(HttpStatusCode.Unauthorized, (await fixture.Client.GetAsync("/api/v1/system/status")).StatusCode);
    }

    [Fact]
    public async Task LastAdministratorIsProtectedAndDeletionRemovesAccount()
    {
        using var fixture = await Fixture.CreateAsync();
        fixture.UseSession();
        Assert.Equal(HttpStatusCode.Conflict, (await fixture.Client.DeleteAsync($"/api/v1/admins/{fixture.Admin.Id}")).StatusCode);
        Assert.Equal(HttpStatusCode.Conflict, (await fixture.Client.PatchAsJsonAsync($"/api/v1/admins/{fixture.Admin.Id}", new { enabled = false })).StatusCode);
        var created = await fixture.Client.PostAsJsonAsync("/api/v1/admins", new { username = "second", password = "x", passwordConfirmation = "x" });
        created.EnsureSuccessStatusCode();
        var second = (await created.Content.ReadFromJsonAsync<JsonElement>()).GetProperty("id").GetGuid();
        (await fixture.Client.DeleteAsync($"/api/v1/admins/{second}")).EnsureSuccessStatusCode();
        using var scope = fixture.Factory.Services.CreateScope();
        Assert.Single(scope.ServiceProvider.GetRequiredService<AppDbContext>().Administrators);
    }

    [Fact]
    public async Task PasswordChangeRequiresOwnAccountCurrentPasswordAndRevokesSessions()
    {
        using var fixture = await Fixture.CreateAsync();
        fixture.UseSession();
        Assert.Equal(HttpStatusCode.BadRequest, (await fixture.Client.PostAsJsonAsync($"/api/v1/admins/{fixture.Admin.Id}/password", new { currentPassword = "password", password = "new", passwordConfirmation = "different" })).StatusCode);
        (await fixture.Client.PostAsJsonAsync($"/api/v1/admins/{fixture.Admin.Id}/password", new { currentPassword = "password", password = "new", passwordConfirmation = "new" })).EnsureSuccessStatusCode();
        Assert.Equal(HttpStatusCode.Unauthorized, (await fixture.Client.GetAsync("/api/v1/system/status")).StatusCode);
        fixture.Client.DefaultRequestHeaders.Clear();
        (await fixture.SignIn("new")).EnsureSuccessStatusCode();
    }

    [Fact]
    public async Task ConsoleEndpointsAreUnavailableThroughHttpAndNetworkHasDisconnectGuard()
    {
        using var fixture = await Fixture.CreateAsync();
        Assert.Equal(HttpStatusCode.Forbidden, (await fixture.Client.PostAsync("/admin/setup-code", null)).StatusCode);
        Assert.Equal(HttpStatusCode.Forbidden, (await fixture.Client.PostAsJsonAsync("/admin/reset-password", new { username = "admin", password = "x" })).StatusCode);
        fixture.UseSession();
        Assert.Equal(HttpStatusCode.OK, (await fixture.Client.GetAsync("/api/v1/system/network")).StatusCode);
        Assert.Equal(HttpStatusCode.BadRequest, (await fixture.Client.PutAsJsonAsync("/api/v1/system/network", new { listenAddresses = InvalidAddresses })).StatusCode);
        Assert.Equal(HttpStatusCode.Conflict, (await fixture.Client.PutAsJsonAsync("/api/v1/system/network", new { listenAddresses = IPv6Loopback })).StatusCode);
        (await fixture.Client.PutAsJsonAsync("/api/v1/system/network", new { listenAddresses = IPv6Loopback, confirmDisconnect = true })).EnsureSuccessStatusCode();
    }

    [Fact]
    public async Task FullScopeAllowsResourceMutationAndReadOnlyRefusesIt()
    {
        using var fixture = await Fixture.CreateAsync();
        foreach (var tokenScope in TokenScopes)
        {
            var (id, _) = await fixture.CreateToken(tokenScope, tokenScope);
            using var scope = fixture.Factory.Services.CreateScope();
            var db = scope.ServiceProvider.GetRequiredService<AppDbContext>();
            var token = (await db.ApiTokens.FindAsync(id))!;
            var context = new DefaultHttpContext { RequestServices = scope.ServiceProvider };
            context.Response.Body = new MemoryStream();
            context.Request.Path = "/api/v1/resource";
            context.Request.Method = "POST";
            context.SetEndpoint(new Endpoint(_ => Task.CompletedTask, new EndpointMetadataCollection(new AccessScope("F")), "test resource"));
            context.Items["fd.token"] = token;
            context.User = new ClaimsPrincipal(new ClaimsIdentity([], "fd-token"));
            var policy = new ScopePolicies(ctx => { ctx.Response.StatusCode = 204; return Task.CompletedTask; });
            await policy.InvokeAsync(context, scope.ServiceProvider.GetRequiredService<AuditWriter>(), scope.ServiceProvider.GetRequiredService<ApiTokenService>());
            Assert.Equal(tokenScope == "Full" ? 204 : 403, context.Response.StatusCode);
        }
    }

    [Fact]
    public async Task ConcurrentRemovalsKeepOneEnabledAdministrator()
    {
        using var fixture = await Fixture.CreateAsync();
        fixture.UseSession();
        var secondResponse = await fixture.Client.PostAsJsonAsync("/api/v1/admins", new { username = "second", password = "x", passwordConfirmation = "x" });
        secondResponse.EnsureSuccessStatusCode();
        var second = (await secondResponse.Content.ReadFromJsonAsync<JsonElement>()).GetProperty("id").GetGuid();
        var results = await Task.WhenAll(fixture.Client.DeleteAsync($"/api/v1/admins/{fixture.Admin.Id}"), fixture.Client.DeleteAsync($"/api/v1/admins/{second}"));
        Assert.Contains(results, response => response.StatusCode == HttpStatusCode.NoContent);
        using var scope = fixture.Factory.Services.CreateScope();
        Assert.Single(scope.ServiceProvider.GetRequiredService<AppDbContext>().Administrators.ToList(), a => a.Enabled);
    }

    internal sealed class Fixture(WebApplicationFactory<Program> factory, HttpClient client, Administrator admin, IssuedSession session) : IDisposable
    {
        public WebApplicationFactory<Program> Factory { get; } = factory;
        public HttpClient Client { get; } = client;
        public Administrator Admin { get; } = admin;
        public static async Task<Fixture> CreateAsync(Action<Microsoft.AspNetCore.Hosting.IWebHostBuilder>? configure = null, bool waitForCore = true)
        {
            var factory = new WebApplicationFactory<Program>().WithWebHostBuilder(builder =>
            {
                builder.UseSetting("Core:Optional", "true");
                builder.UseSetting("Database:Path", Path.Combine(Path.GetTempPath(), $"fd-auth-{Guid.NewGuid():N}", "app.db"));
                builder.UseSetting("Kestrel:HttpPort", "0");
                builder.UseSetting("Logging:LogLevel:Default", "Warning");
                configure?.Invoke(builder);
            });
            var client = factory.CreateClient(new WebApplicationFactoryClientOptions { HandleCookies = false });
            using var scope = factory.Services.CreateScope();
            var admin = await scope.ServiceProvider.GetRequiredService<AdminService>().CreateAsync("admin", "password");
            await scope.ServiceProvider.GetRequiredService<SetupCodeService>().MarkCompleteAsync();
            var session = await scope.ServiceProvider.GetRequiredService<SessionService>().CreateAsync(admin, "127.0.0.1", "test");
            var health = factory.Services.GetRequiredService<FileDock.Management.Core.CoreHealthMonitor>();
            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
            while (waitForCore && !health.Current.CanMutate) { await Task.Delay(10, timeout.Token); }
            return new Fixture(factory, client, admin, session);
        }
        public void UseSession(bool csrf = true)
        {
            Client.DefaultRequestHeaders.Clear();
            Client.DefaultRequestHeaders.Add("Cookie", $"fd_http_session={session.Token}");
            if (csrf)
            {
                Client.DefaultRequestHeaders.Add("X-CSRF-Token", session.Csrf);
            }
        }
        public void UseToken(string token)
        {
            Client.DefaultRequestHeaders.Clear();
            Client.DefaultRequestHeaders.Authorization = new("Bearer", token);
        }
        public Task<HttpResponseMessage> SignIn(string password) => Client.PostAsJsonAsync("/api/v1/auth/sign-in", new { username = "admin", password });
        public async Task<(Guid, string)> CreateToken(string name, string tokenScope)
        {
            UseSession();
            var result = await Client.PostAsJsonAsync("/api/v1/tokens", new { name, scope = tokenScope });
            result.EnsureSuccessStatusCode();
            var body = await result.Content.ReadFromJsonAsync<JsonElement>();
            return (body.GetProperty("id").GetGuid(), body.GetProperty("token").GetString()!);
        }
        public void Dispose() { Client.Dispose(); Factory.Dispose(); }
    }
}
