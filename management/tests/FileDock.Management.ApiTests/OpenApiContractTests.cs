using System.Text.RegularExpressions;
using Microsoft.AspNetCore.Http.Metadata;
using Microsoft.AspNetCore.Routing;
using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using FileDock.Management.Domain.Setup;
using Microsoft.AspNetCore.Mvc.Testing;
using Microsoft.Extensions.DependencyInjection;

namespace FileDock.Management.ApiTests;

public sealed class OpenApiContractTests
{
    [Fact]
    public async Task GeneratedDocumentUsesRegisteredApiRoutesAndProblemResponses()
    {
        await using var factory = new WebApplicationFactory<Program>().WithWebHostBuilder(builder =>
        {
            builder.UseSetting("Core:Optional", "true");
            builder.UseSetting("Database:Path", Path.Combine(Path.GetTempPath(), $"fd-openapi-{Guid.NewGuid():N}.db"));
            builder.UseSetting("Kestrel:HttpPort", "0");
            builder.UseSetting("Logging:LogLevel:Default", "Warning");
        });
        using var client = factory.CreateClient();
        Assert.Equal(HttpStatusCode.Locked, (await client.GetAsync("/api/v1/openapi.json")).StatusCode);
        string code;
        using (var scope = factory.Services.CreateScope()) { code = await scope.ServiceProvider.GetRequiredService<SetupCodeService>().IssueAsync(); }
        var complete = await client.PostAsJsonAsync("/api/v1/setup/complete", new { code, applianceName = "contract", adminUsername = "root", adminPassword = "fixture", adminPasswordConfirmation = "fixture" });
        complete.EnsureSuccessStatusCode();
        using var setup = JsonDocument.Parse(await complete.Content.ReadAsStringAsync());
        var cookie = complete.Headers.GetValues("Set-Cookie").First(h => h.StartsWith("fd_http_csrf=", StringComparison.Ordinal)).Split(';')[0].Split('=', 2)[1];
        client.DefaultRequestHeaders.Add("X-CSRF-Token", cookie);
        (await client.PostAsJsonAsync("/api/v1/setup/recovery-key/confirm", new { keyId = setup.RootElement.GetProperty("keyId").GetString() })).EnsureSuccessStatusCode();
        var response = await client.GetAsync("/api/v1/openapi.json");
        response.EnsureSuccessStatusCode();
        var json = await response.Content.ReadAsStringAsync();
        await File.WriteAllTextAsync(Path.Combine(Path.GetTempPath(), "filedock-phase13-openapi.json"), json);
        using var doc = JsonDocument.Parse(json);
        Assert.StartsWith("3.1", doc.RootElement.GetProperty("openapi").GetString());
        var paths = doc.RootElement.GetProperty("paths");
        Assert.True(paths.TryGetProperty("/api/v1/connector-routes", out _));
        Assert.True(paths.TryGetProperty("/api/v1/state-protection", out _));
        foreach (var path in paths.EnumerateObject()) { Assert.StartsWith("/api/v1/", path.Name); }
        var actual = new SortedDictionary<string, string>(StringComparer.Ordinal);
        foreach (var path in paths.EnumerateObject())
        {
            foreach (var method in path.Value.EnumerateObject())
            {
                if (method.Name is not ("get" or "post" or "put" or "patch" or "delete" or "head" or "options")) { continue; }
                actual[$"{method.Name.ToUpperInvariant()} {path.Name}"] = string.Join(",", method.Value.GetProperty("responses").EnumerateObject().Select(r => r.Name).Where(c => int.TryParse(c, out var code) && code >= 400).Order(StringComparer.Ordinal));
            }
        }
        var registered = factory.Services.GetRequiredService<EndpointDataSource>().Endpoints.OfType<RouteEndpoint>()
            .Where(e => e.RoutePattern.RawText?.StartsWith("/api/v1/", StringComparison.Ordinal) == true && e.Metadata.GetMetadata<IExcludeFromDescriptionMetadata>()?.ExcludeFromDescription != true)
            .SelectMany(e => (e.Metadata.GetMetadata<HttpMethodMetadata>()?.HttpMethods ?? []).Select(method => $"{method} {Regex.Replace(e.RoutePattern.RawText!, @":[^}]+(?=})", "").Replace("{**", "{", StringComparison.Ordinal).Replace("{*", "{", StringComparison.Ordinal).TrimEnd('/')}"))
            .Order(StringComparer.Ordinal).ToArray();
        Assert.Equal(registered, actual.Keys);
        var root = new DirectoryInfo(AppContext.BaseDirectory);
        while (root is not null && !File.Exists(Path.Combine(root.FullName, "specs/001-core-os-foundation/contracts/management-api.md"))) { root = root.Parent; }
        Assert.NotNull(root);
        var contract = await File.ReadAllTextAsync(Path.Combine(root.FullName, "specs/001-core-os-foundation/contracts/management-api.md"));
        var expected = Regex.Matches(contract, @"\| `(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS) (/api/v1/[^`]+)` \| `([0-9,]+)` \|")
            .ToDictionary(m => $"{m.Groups[1].Value} {m.Groups[2].Value}", m => m.Groups[3].Value, StringComparer.Ordinal);
        Assert.NotEmpty(expected);
        Assert.Equal(expected.OrderBy(x => x.Key, StringComparer.Ordinal), actual);
        var responses = paths.GetProperty("/api/v1/connector-routes").GetProperty("put").GetProperty("responses");
        foreach (var status in new[] { "401", "403", "409", "422", "423", "503", "507" })
        {
            Assert.True(responses.GetProperty(status).GetProperty("content").TryGetProperty("application/problem+json", out _));
        }
    }
}
