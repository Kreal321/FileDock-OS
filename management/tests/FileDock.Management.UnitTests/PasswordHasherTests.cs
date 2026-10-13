using FileDock.Management.Domain.Security;

namespace FileDock.Management.UnitTests;

public sealed class PasswordHasherTests
{
    [Fact]
    public void RejectsEmptyPasswords()
    {
        Assert.Throws<ArgumentException>(() => PasswordHasher.Hash(""));
    }

    [Fact]
    public void AllowsShortNonEmptyPasswords()
    {
        Assert.True(PasswordHasher.Verify("x", PasswordHasher.Hash("x")));
    }

    [Fact]
    public void RoundTrips()
    {
        var encoded = PasswordHasher.Hash("correct horse");
        Assert.True(PasswordHasher.Verify("correct horse", encoded));
        Assert.False(PasswordHasher.Verify("wrong password", encoded));
    }
}
