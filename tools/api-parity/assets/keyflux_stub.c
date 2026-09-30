/* No-op KeyFlux.exe stub for the api-parity sandbox. See assets/README in
 * tools/api-parity/README.md ("沙箱 stub") for why it must exist.
 *
 * REBUILD (any C compiler works; the asset is committed, the tool never compiles):
 *   gcc -Os -s -static -o tools/api-parity/assets/KeyFlux-stub.exe \
 *       tools/api-parity/assets/keyflux_stub.c
 * Verify: run it -> exit code 0, no output, no window.
 * Extension .c (not .txt) so editors/linters treat it as C; it is not compiled by any
 * build of the product or the harness. */
int main(void) { return 0; }
