using System.Net;
using System.Net.Http.Json;
using FileDock.Management.Core;
using Grpc.Core;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
namespace FileDock.Management.ApiTests;

public sealed class EventApiTests
{
    [Fact]
    public async Task ManagementStartsWithCoreDownAndStillReturnsAnAuthenticatedStatus()
    {
        var gateway=new ToggleGateway {Down=true};
        using var fixture=await AuthTests.Fixture.CreateAsync(builder=>builder.ConfigureServices(services=>{services.RemoveAll<ICoreGateway>();services.AddSingleton<ICoreGateway>(gateway);}),waitForCore:false);
        fixture.UseSession();
        var response=await fixture.Client.GetAsync("/api/v1/system/status");
        Assert.Equal(HttpStatusCode.OK,response.StatusCode);
        var body=await response.Content.ReadFromJsonAsync<System.Text.Json.JsonElement>();
        Assert.Contains(body.GetProperty("components").EnumerateArray(),c=>c.GetProperty("id").GetString()=="management");
    }
    [Fact]
    public async Task PublicHealthButProtectedEventsAndOperations()
    {
        using var fixture = await AuthTests.Fixture.CreateAsync();
        Assert.Equal(HttpStatusCode.OK, (await fixture.Client.GetAsync("/api/v1/health")).StatusCode);
        Assert.Equal(HttpStatusCode.Unauthorized, (await fixture.Client.GetAsync("/api/v1/operations")).StatusCode);
        Assert.Equal(HttpStatusCode.Unauthorized, (await fixture.Client.PostAsync("/hubs/events/negotiate?negotiateVersion=1", null)).StatusCode);
        fixture.UseSession();
        Assert.Equal(HttpStatusCode.OK, (await fixture.Client.GetAsync("/api/v1/operations")).StatusCode);
        Assert.Equal(HttpStatusCode.NotFound, (await fixture.Client.GetAsync("/api/v1/operations/missing")).StatusCode);
        Assert.Equal(HttpStatusCode.OK, (await fixture.Client.PostAsync("/hubs/events/negotiate?negotiateVersion=1", null)).StatusCode);
        fixture.Client.DefaultRequestHeaders.Add("Origin", "http://foreign.example");
        Assert.Equal(HttpStatusCode.Forbidden, (await fixture.Client.PostAsync("/hubs/events/negotiate?negotiateVersion=1", null)).StatusCode);
        fixture.Client.DefaultRequestHeaders.Remove("Origin");
        var (_, token) = await fixture.CreateToken("observer", "ReadOnly"); fixture.UseToken(token);
        Assert.Equal(HttpStatusCode.OK, (await fixture.Client.PostAsync("/hubs/events/negotiate?negotiateVersion=1", null)).StatusCode);
        Assert.Equal(HttpStatusCode.Forbidden, (await fixture.Client.PostAsync("/api/v1/operations/reconcile", null)).StatusCode);
    }
    [Fact]
    public async Task CoreLossAndVersionMismatchBlockChangesAndRecover()
    {
        var gateway = new ToggleGateway();
        using var fixture = await AuthTests.Fixture.CreateAsync(builder => builder.ConfigureServices(services => { services.RemoveAll<ICoreGateway>(); services.AddSingleton<ICoreGateway>(gateway); }));
        fixture.UseSession(); var monitor = fixture.Factory.Services.GetRequiredService<CoreHealthMonitor>();
        await WaitAsync(() => monitor.Current.CanMutate);
        gateway.Down = true; await WaitAsync(() => monitor.Current.State == "Unreachable");
        Assert.Equal(HttpStatusCode.OK, (await fixture.Client.GetAsync("/api/v1/system/status")).StatusCode);
        Assert.Equal(HttpStatusCode.ServiceUnavailable, (await fixture.Client.PostAsJsonAsync("/api/v1/tokens", new { name = "blocked", scope = "Full" })).StatusCode);
        gateway.Down = false; gateway.Version = "2.0"; await WaitAsync(() => monitor.Current.State == "Incompatible");
        Assert.Equal(HttpStatusCode.ServiceUnavailable, (await fixture.Client.PostAsync("/api/v1/operations/reconcile", null)).StatusCode);
        gateway.Version = "1.0"; await WaitAsync(() => monitor.Current.CanMutate);
        Assert.Equal(HttpStatusCode.Accepted, (await fixture.Client.PostAsync("/api/v1/operations/reconcile", null)).StatusCode);
    }
    private static async Task WaitAsync(Func<bool> condition) { using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(6)); while (!condition()) { await Task.Delay(20, timeout.Token); } }
    private sealed class ToggleGateway : ICoreGateway
    {
        public bool Down; public string Version = "1.0"; private readonly LocalCoreGateway _inner = new();
        public Task ApplyApplianceNameAsync(string name, string actor, string id, CancellationToken ct = default) => _inner.ApplyApplianceNameAsync(name, actor, id, ct);
        public Task<GeneratedRecoveryKeyDto> GenerateRecoveryKeyAsync(string actor, string id, CancellationToken ct = default) => _inner.GenerateRecoveryKeyAsync(actor, id, ct);
        public Task ConfirmRecoveryKeyAsync(string id, string actor, string requestId, CancellationToken ct = default) => _inner.ConfirmRecoveryKeyAsync(id, actor, requestId, ct);
        public async Task<SystemStatusDto> GetSystemStatusAsync(CancellationToken ct = default) { if (Down) { throw new RpcException(new Status(StatusCode.Unavailable, "down")); } return (await _inner.GetSystemStatusAsync(ct)) with { InterfaceVersion = Version }; }
    }
}
