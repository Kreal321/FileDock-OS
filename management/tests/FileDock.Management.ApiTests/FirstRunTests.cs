using System.Net;
using System.Net.Http.Json;
using Microsoft.AspNetCore.Mvc.Testing;

namespace FileDock.Management.ApiTests;

public sealed class FirstRunTests : IClassFixture<WebApplicationFactory<Program>>
{
    private readonly WebApplicationFactory<Program> _factory;

    public FirstRunTests(WebApplicationFactory<Program> factory)
    {
        _factory = factory.WithWebHostBuilder(builder =>
        {
            builder.UseSetting("Core:Optional", "true");
            builder.UseSetting("Database:Path", Path.Combine(Path.GetTempPath(), $"fd-api-{Guid.NewGuid():N}.db"));
            builder.UseSetting("Kestrel:HttpsPort", "0");
        });
    }

    [Fact]
    public async Task OtherRoutesAreLockedUntilSetupCompletes()
    {
        var client = _factory.CreateClient();
        var locked = await client.GetAsync("/api/v1/system/status");
        Assert.Equal(HttpStatusCode.Locked, locked.StatusCode);

        var health = await client.GetAsync("/health");
        Assert.True(health.IsSuccessStatusCode);
    }

    [Fact]
    public async Task SetupCompletesOnlyAfterRecoveryKeyConfirmation()
    {
        var client = _factory.CreateClient();
        var issued = await client.PostAsync("/admin/setup-code", null);
        issued.EnsureSuccessStatusCode();
        var code = (await issued.Content.ReadFromJsonAsync<CodeResponse>())!.Code;

        var complete = await client.PostAsJsonAsync("/api/v1/setup/complete", new
        {
            code,
            applianceName = "harbor",
            adminUsername = "alice",
            adminPassword = "correct horse",
        });
        complete.EnsureSuccessStatusCode();
        var generated = await complete.Content.ReadFromJsonAsync<GeneratedResponse>();
        Assert.False(string.IsNullOrEmpty(generated?.RecoveryKey));

        var stillLocked = await client.GetAsync("/api/v1/system/status");
        Assert.Equal(HttpStatusCode.Locked, stillLocked.StatusCode);

        var groups = generated!.RecoveryKey.Split('-');
        var confirm = generated.ConfirmGroupIndexes.ToDictionary(i => i, i => groups[i]);
        using var confirmRequest = new HttpRequestMessage(HttpMethod.Post, "/api/v1/setup/recovery-key/confirm")
        {
            Content = JsonContent.Create(new { keyId = generated.KeyId, groups = confirm }),
        };
        confirmRequest.Headers.TryAddWithoutValidation("X-CSRF-Token", ReadSetCookie(complete, "fd_csrf"));
        var confirmed = await client.SendAsync(confirmRequest);
        confirmed.EnsureSuccessStatusCode();

        var status = await client.GetAsync("/api/v1/system/status");
        Assert.True(status.IsSuccessStatusCode);
    }

    private static string ReadSetCookie(HttpResponseMessage response, string name)
    {
        if (!response.Headers.TryGetValues("Set-Cookie", out var cookies))
        {
            return "";
        }

        foreach (var header in cookies)
        {
            var trimmed = header.Trim();
            if (trimmed.StartsWith(name + "=", StringComparison.Ordinal))
            {
                return trimmed.Split(';', 2)[0][(name.Length + 1)..];
            }
        }

        return "";
    }

    private sealed record CodeResponse(string Code);
    private sealed record GeneratedResponse(string KeyId, string RecoveryKey, uint[] ConfirmGroupIndexes);
}
