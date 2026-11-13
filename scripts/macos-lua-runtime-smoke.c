/* Load the prepared archive in a standalone LuaJIT process without using ambient Lua paths.
 * 在独立 LuaJIT 进程中加载已准备的归档，不使用环境中的 Lua 路径。 */
#include <dlfcn.h>
#include <stdio.h>

/* LuaState is the opaque Lua 5.1 state owned by the archive's bundled LuaJIT.
 * LuaState 是归档自带 LuaJIT 持有的 Lua 5.1 不透明状态。 */
typedef struct lua_State LuaState;

/* argv contains the packaged LuaJIT path and the packaged lua_packages path.
 * argv 包含随包 LuaJIT 路径与随包 lua_packages 路径。
 * Returns zero after successfully loading the native modules and exercising OpenSSL and cURL.
 * 成功加载原生模块并验证 OpenSSL 与 cURL 后返回零。 */
int main(int argc, char **argv) {
    if (argc != 3) {
        return 1;
    }
    /* runtime exports Lua symbols globally for Mach-O modules built with dynamic lookup.
     * runtime 全局导出 Lua 符号，供采用动态查找方式构建的 Mach-O 模块使用。 */
    void *runtime = dlopen(argv[1], RTLD_NOW | RTLD_GLOBAL);
    if (!runtime) {
        fprintf(stderr, "%s\n", dlerror());
        return 1;
    }
    /* new_state creates an isolated Lua state through the bundled Lua 5.1 API.
     * new_state 通过随包 Lua 5.1 接口创建隔离 Lua 状态。 */
    LuaState *(*new_state)(void) = dlsym(runtime, "luaL_newstate");
    /* open_libraries installs the Lua package searchers.
     * open_libraries 安装 Lua 包搜索器。 */
    void (*open_libraries)(LuaState *) = dlsym(runtime, "luaL_openlibs");
    /* push_string passes the exact package directory to the smoke chunk.
     * push_string 将精确的包目录传给冒烟测试代码。 */
    void (*push_string)(LuaState *, const char *) = dlsym(runtime, "lua_pushstring");
    /* load_string compiles the smoke chunk.
     * load_string 编译冒烟测试代码。 */
    int (*load_string)(LuaState *, const char *) = dlsym(runtime, "luaL_loadstring");
    /* call executes the chunk and leaves any loader error on the Lua stack.
     * call 执行代码并将加载错误留在 Lua 栈上。 */
    int (*call)(LuaState *, int, int, int) = dlsym(runtime, "lua_pcall");
    /* to_string reads a diagnostic from the Lua stack.
     * to_string 从 Lua 栈读取诊断。 */
    const char *(*to_string)(LuaState *, int, size_t *) = dlsym(runtime, "lua_tolstring");
    /* close_state releases the Lua state after the verification.
     * close_state 在验证后释放 Lua 状态。 */
    void (*close_state)(LuaState *) = dlsym(runtime, "lua_close");
    if (!new_state || !open_libraries || !push_string || !load_string || !call || !to_string || !close_state) {
        fputs("Bundled LuaJIT is missing required Lua 5.1 exports.\n", stderr);
        return 1;
    }
    /* lua is a disposable state with no access to the application's settings or data. */
    /* lua 是不访问应用设置或数据的临时状态。 */
    LuaState *lua = new_state();
    if (!lua) {
        return 1;
    }
    open_libraries(lua);
    /* smoke pins both search paths and exercises a cURL easy handle without making a request.
     * smoke 固定两个模块搜索路径，并在不发起请求的情况下验证 cURL easy 句柄。 */
    const char *smoke =
        "local root = ...\n"
        "package.path = root .. '/share/lua/?.lua;' .. root .. '/share/lua/?/init.lua'\n"
        "package.cpath = root .. '/lib/lua/?.dylib;' .. root .. '/lib/lua/?/init.dylib'\n"
        "for _, module in ipairs({'cjson', 'iconv', 'lfs', 'lpeg', 'lua-utf8', 'lxp', "
        "'mime.core', 'rex_pcre2', 'socket.core', 'socket.serial', 'socket.unix', "
        "'ssl', 'yaml', 'zip', 'zlib'}) do require(module) end\n"
        "assert(#require('openssl.rand').bytes(32) == 32)\n"
        "assert(require('openssl.cipher').new('aes-256-gcm'))\n"
        "local handle = assert(require('lcurl.safe').easy({url = 'https://context7.com', "
        "httpget = true, followlocation = false, ssl_verifypeer = true, ssl_verifyhost = true}))\n"
        "handle:close()\n";
    /* status is the Lua compilation or execution result. */
    /* status 是 Lua 编译或执行的结果。 */
    int status = load_string(lua, smoke);
    if (status == 0) {
        push_string(lua, argv[2]);
        status = call(lua, 1, 0, 0);
    }
    if (status != 0) {
        /* error is the native loader diagnostic, when the failure supplied a string.
         * error 是原生加载器返回的字符串诊断。 */
        const char *error = to_string(lua, -1, NULL);
        fputs(error ? error : "Lua raised a non-string error.", stderr);
        fputc('\n', stderr);
    }
    close_state(lua);
    return status != 0;
}
