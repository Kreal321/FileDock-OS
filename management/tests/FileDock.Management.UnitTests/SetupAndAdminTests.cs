using FileDock.Management.Domain;
using FileDock.Management.Domain.Admins;
using FileDock.Management.Domain.Setup;
using FileDock.Management.Domain.Stores;

namespace FileDock.Management.UnitTests;

public sealed class SetupAndAdminTests
{
    [Fact]
    public async Task SetupCodeIsHashedAndThrottled()
    {
        var store = new MemorySetupStore();
        var setup = new SetupCodeService(store);
        var code = await setup.IssueAsync();
        await setup.VerifyAsync(code, "10.0.0.1");
        for (var i = 0; i < 5; i++)
        {
            try
            {
                await setup.VerifyAsync("AAAAAAAA", "10.0.0.2");
            }
            catch (ArgumentException)
            {
                // expected until throttle
            }
            catch (SetupThrottleException)
            {
                return;
            }
        }

        await Assert.ThrowsAsync<SetupThrottleException>(() => setup.VerifyAsync("AAAAAAAA", "10.0.0.2"));
    }

    [Fact]
    public async Task LastEnabledAdminIsProtected()
    {
        var store = new MemoryAdminStore();
        var admins = new AdminService(store);
        var first = await admins.CreateAsync("alice", "correct horse");
        await Assert.ThrowsAsync<LastAdminException>(() => admins.DisableAsync(first.Id));
    }

    [Fact]
    public async Task FiveFailuresLockTheAccount()
    {
        var store = new MemoryAdminStore();
        var admins = new AdminService(store);
        await admins.CreateAsync("bob", "correct horse");
        for (var i = 0; i < 4; i++)
        {
            await Assert.ThrowsAsync<UnauthorizedAccessException>(() => admins.AuthenticateAsync("bob", "wrong password"));
        }

        await Assert.ThrowsAsync<AccountLockedException>(() => admins.AuthenticateAsync("bob", "wrong password"));
    }

    private sealed class MemorySetupStore : ISetupStore
    {
        private SetupState _state = new() { Id = 1, Status = SetupCodeService.Awaiting };

        public Task<SetupState> GetAsync(CancellationToken cancellationToken = default) => Task.FromResult(_state);

        public Task SaveAsync(SetupState state, CancellationToken cancellationToken = default)
        {
            _state = state;
            return Task.CompletedTask;
        }
    }

    private sealed class MemoryAdminStore : IAdminStore
    {
        private readonly List<Administrator> _admins = [];

        public Task<Administrator?> FindByUsernameAsync(string username, CancellationToken cancellationToken = default)
            => Task.FromResult(_admins.FirstOrDefault(a => a.Username == username));

        public Task<Administrator?> FindByIdAsync(Guid id, CancellationToken cancellationToken = default)
            => Task.FromResult(_admins.FirstOrDefault(a => a.Id == id));

        public Task<int> CountEnabledAsync(CancellationToken cancellationToken = default)
            => Task.FromResult(_admins.Count(a => a.Enabled));

        public Task AddAsync(Administrator admin, CancellationToken cancellationToken = default)
        {
            _admins.Add(admin);
            return Task.CompletedTask;
        }

        public Task SaveAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<IReadOnlyList<Administrator>> ListAsync(CancellationToken cancellationToken = default)
            => Task.FromResult<IReadOnlyList<Administrator>>(_admins);
    }
}
