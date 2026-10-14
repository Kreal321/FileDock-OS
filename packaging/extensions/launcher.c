/* First-party Docker entrypoint: keeps the per-instance token out of Engine's persisted Env. */
#define _GNU_SOURCE
#define _DARWIN_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#include <fcntl.h>
#include <unistd.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
int main(int argc, char **argv) {
    const char *path = getenv("FD_EXTENSION_TOKEN_FILE");
    if (!path || argc < 2) { fputs("Extension launch configuration is incomplete\n", stderr); return 1; }
    int fd = open(path, O_RDONLY | O_NOFOLLOW);
    if (fd < 0) { fputs("Extension token is unavailable\n", stderr); return 1; }
    char token[257]; ssize_t n = read(fd, token, 256); close(fd);
    if (n <= 0 || n > 128) { fputs("Extension token is invalid\n", stderr); return 1; }
    token[n] = 0;
    if (setenv("FD_EXTENSION_TOKEN", token, 1) != 0) { return 1; }
    memset(token, 0, sizeof token);
    unsetenv("FD_EXTENSION_TOKEN_FILE");
    execvp(argv[1], argv + 1);
    fputs("Extension executable failed\n", stderr); return 1;
}
