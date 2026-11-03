// Package the verified Windows x64 release distribution into an unsigned Inno Setup installer.
// 将已验证的 Windows x64 正式版发行物打包为未签名的 Inno Setup 安装程序。
import { spawn } from 'node:child_process';
import { access, mkdir, readdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { buildInputPaths } from './build-input-paths.mjs';
import { managedRuntimePlatformKey } from './desktop-runtime.mjs';
import { obtainLockedInput } from './locked-download.mjs';
import { inspectReleaseDistribution } from './release-package.mjs';

// The repository is resolved from this script rather than the caller's working directory.
// 仓库由脚本位置确定，不依赖调用方工作目录。
const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

// installerAppId identifies every Vulcan Code installation for upgrades and uninstall; never change it.
// installerAppId 标识每一次 Vulcan Code 安装，用于升级与卸载；不得修改。
const installerAppId = '4AA6A407-B39F-4AF7-9F9B-167D14D64368';

// installerAssetName names the setup program beside the release archive of the same version and platform.
// installerAssetName 为安装程序命名，与同版本、同平台的发行归档并列。
export function installerAssetName(layout, version) {
  return `vulcan-code-${version}-${managedRuntimePlatformKey(layout)}-setup`;
}

// The multi-version layout below the install directory (plan section 4); `vulcan-install-core`
// (`install_root.rs`) and the launcher (`apps/vulcan-code-launcher`) read the same names.
// 安装目录下的多版本布局（方案第四节）；`vulcan-install-core`（`install_root.rs`）与启动器
// （`apps/vulcan-code-launcher`）读取相同的名称。
export const installLayout = Object.freeze({
  // launcher is the stable desktop entry every shortcut and taskbar pin points at.
  // launcher 是所有快捷方式与任务栏固定项指向的稳定桌面入口。
  launcher: 'VulcanCode.exe',
  // cli holds only the PATH forwarder, so adding it to PATH exposes nothing else.
  // cli 只包含 PATH 转发器，因此加入 PATH 不会暴露其他程序。
  cli: 'cli',
  forwarder: 'vulcan-tui.exe',
  versions: 'versions',
  updates: 'updates',
  current: 'current.json',
});

// launcherSources maps each installed launcher program to the shipped binary it is copied from.
// launcherSources 将每个已安装的启动程序映射到其复制来源的分发二进制。
const launcherSources = Object.freeze([
  { binary: 'vulcan-code-launcher', destination: installLayout.launcher },
  { binary: 'vulcan-tui-forwarder', destination: `${installLayout.cli}\\${installLayout.forwarder}` },
]);

// releaseVersionPattern accepts the version forms release archives use; it also names a directory.
// releaseVersionPattern 接受发行归档使用的版本形式；它同时用作目录名。
const releaseVersionPattern = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.]+)?$/;

// issValue rejects characters that would end an Inno Setup quoted value, so paths cannot inject directives.
// issValue 拒绝会结束 Inno Setup 引号值的字符，避免路径注入指令。
function issValue(value) {
  if (/["\r\n]/.test(value)) throw new Error(`Value cannot be written into an Inno Setup script: ${value}`);
  return value;
}

// pascalString quotes a value for the [Code] section; doubling the quote is Pascal's only escape.
// pascalString 为 [Code] 节引用一个值；Pascal 唯一的转义是将引号加倍。
function pascalString(value) {
  if (/[\r\n]/.test(value)) throw new Error(`Value cannot be written into Pascal code: ${value}`);
  return `'${value.replaceAll("'", "''")}'`;
}

// desktopShortcutIdentity checks the release notification identity the desktop process applies to itself.
// desktopShortcutIdentity 检查桌面进程为自身设置的正式版通知身份。
// Shortcuts carry the same AppUserModelID, so the taskbar groups the window with them and pins the launcher.
// 快捷方式携带相同的 AppUserModelID，使任务栏将窗口与其归为一组，并固定启动器。
function desktopShortcutIdentity(identity) {
  const { name, appUserModelId, toastActivatorClsid } = identity ?? {};
  if (typeof name !== 'string' || !/^[A-Za-z0-9 ]+$/.test(name)) throw new Error(`Invalid desktop application name: ${name}.`);
  if (typeof appUserModelId !== 'string' || !/^[A-Za-z0-9.]+$/.test(appUserModelId) || appUserModelId.includes('.debug.')) {
    throw new Error(`The desktop executable has no release AppUserModelID: ${appUserModelId}.`);
  }
  const clsid = /^\{([0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12})\}$/i.exec(toastActivatorClsid ?? '');
  if (!clsid) throw new Error(`Invalid toast activator CLSID: ${toastActivatorClsid}.`);
  return { name, appUserModelId, clsid: clsid[1].toUpperCase() };
}

// innoSetupScript renders the installer definition for one verified distribution.
// innoSetupScript 为一份已验证发行物生成安装程序定义。
// The distribution is installed as versions\<version>; the launcher, the PATH forwarder and current.json sit
// beside it. entries are the distribution's top-level names, which are also the top-level release files of
// the former single-directory layout, so installing over that layout removes exactly those, never user data.
// identity is the desktop executable's release notification identity.
// 发行物安装为 versions\<版本>；启动器、PATH 转发器与 current.json 位于其旁。entries 是发行物的顶层名称，
// 同时也是旧单目录布局的顶层发行文件，因此覆盖安装旧布局时只删除这些条目，绝不删除用户数据。identity 是
// 桌面可执行文件的正式版通知身份。
export function innoSetupScript({ root, layout, version, entries, outputDir, icon, identity }) {
  if (layout.platform !== 'win32' || layout.architecture !== 'x64') {
    throw new Error(`Only a Windows x64 distribution can be packaged; found ${layout.platform}-${layout.architecture}.`);
  }
  if (!releaseVersionPattern.test(version)) throw new Error(`Invalid release version: ${version}.`);
  if (entries.length === 0) throw new Error(`The distribution in ${root} is empty.`);
  // A distribution entry with a layout name would be deleted by the legacy cleanup of every later install.
  // 与布局名称相同的发行条目会在之后每次安装的旧布局清理中被删除。
  const reserved = new Set(Object.values(installLayout).map(name => name.toLowerCase()));
  const clash = entries.find(entry => reserved.has(entry.toLowerCase()) || /^unins\d+\./i.test(entry));
  if (clash) throw new Error(`The distribution entry ${clash} collides with the install layout.`);
  const { name: desktopName, appUserModelId, clsid } = desktopShortcutIdentity(identity);
  const { launcher, cli, forwarder, versions, updates, current } = installLayout;
  const versionDir = `${versions}\\${issValue(version)}`;
  const legacy = entries.map(entry => `Type: filesandordirs; Name: "{app}\\${issValue(entry)}"`).join('\n');
  const launcherFiles = launcherSources.map(({ binary, destination }) => {
    const slash = destination.lastIndexOf('\\');
    const directory = slash === -1 ? '{app}' : `{app}\\${destination.slice(0, slash)}`;
    return `Source: "${issValue(root)}\\bin\\${binary}.exe"; DestDir: "${directory}"; DestName: "${destination.slice(slash + 1)}"; Flags: ignoreversion`;
  }).join('\n');
  const shortcut = `Filename: "{app}\\${launcher}"; AppUserModelID: "${appUserModelId}"; AppUserModelToastActivatorCLSID: "${clsid}"`;
  const currentJson = JSON.stringify({ version, path: `${versions}\\${version}` });
  return `; Generated by scripts/windows-installer.mjs; do not edit.
[Setup]
AppId={{${installerAppId}}
AppName=Vulcan Code
AppVersion=${issValue(version)}
AppVerName=Vulcan Code ${issValue(version)}
AppPublisher=OpenVulcan
DefaultDirName={autopf}\\VulcanCode
DefaultGroupName=Vulcan Code
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
Compression=lzma2/max
SolidCompression=yes
LZMAUseSeparateProcess=yes
CloseApplications=yes
ChangesEnvironment=yes
WizardStyle=modern
SetupIconFile=${issValue(icon)}
UninstallDisplayIcon={app}\\${launcher}
OutputDir=${issValue(outputDir)}
OutputBaseFilename=${issValue(installerAssetName(layout, version))}

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "addtopath"; Description: "Add vulcan-tui to PATH for the current user"; GroupDescription: "Command line:"

[InstallDelete]
; Top-level release files of the former single-directory layout.
${legacy}
; A reinstall of the same version starts from a clean version directory.
Type: filesandordirs; Name: "{app}\\${versionDir}"
; Launchers moved aside while they were running; those still in use stay until the next install.
Type: files; Name: "{app}\\${launcher}.*.old"
Type: files; Name: "{app}\\${cli}\\${forwarder}.*.old"

[Files]
Source: "${issValue(root)}\\*"; DestDir: "{app}\\${versionDir}"; Flags: ignoreversion recursesubdirs createallsubdirs
${launcherFiles}

[Icons]
Name: "{autoprograms}\\Vulcan Code"; ${shortcut}
Name: "{autodesktop}\\Vulcan Code"; ${shortcut}; Tasks: desktopicon

[Registry]
; Electron registers the toast activator for the running desktop executable; remove it with the program.
Root: HKCU; Subkey: "Software\\Classes\\CLSID\\{{${clsid}}"; Flags: uninsdeletekey dontcreatekey

[Run]
Filename: "{app}\\${launcher}"; Description: "{cm:LaunchProgram,Vulcan Code}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: filesandordirs; Name: "{app}\\${versions}"
Type: filesandordirs; Name: "{app}\\${updates}"
Type: filesandordirs; Name: "{app}\\${cli}"
Type: files; Name: "{app}\\${current}"
Type: files; Name: "{app}\\${current}.partial"
Type: files; Name: "{app}\\${launcher}.*.old"
; Electron keeps its own Start menu shortcut pointing at the running desktop executable.
Type: files; Name: "{userprograms}\\${desktopName}.lnk"
${legacy}

[Code]
const
  MOVEFILE_REPLACE_EXISTING = 1;
  MOVEFILE_WRITE_THROUGH = 8;
  // TFileStream mode fmOpenWrite or fmShareDenyNone.
  OPEN_WRITE_SHARE_ALL = $41;
  CurrentJson = ${pascalString(currentJson)};
  InstalledVersion = ${pascalString(version)};

var
  // Launchers renamed by PrepareToInstall, restored when the installation does not finish.
  MovedAside: TArrayOfString;
  Installed: Boolean;

function MoveFileEx(ExistingFileName, NewFileName: String; Flags: Cardinal): Boolean;
  external 'MoveFileExW@kernel32.dll stdcall';

function CliDir: String;
begin
  Result := ExpandConstant('{app}\\${cli}');
end;

// A running launcher cannot be overwritten but can be renamed, so a TUI left open in a terminal never
// blocks an install; the renamed copy is deleted by a later install or by uninstall.
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Targets: TArrayOfString;
  Aside: String;
  I: Integer;
begin
  Result := '';
  SetArrayLength(Targets, 2);
  Targets[0] := ExpandConstant('{app}\\${launcher}');
  Targets[1] := CliDir + '\\${forwarder}';
  for I := 0 to GetArrayLength(Targets) - 1 do
    if FileExists(Targets[I]) then
    begin
      Aside := Targets[I] + '.' + GetDateTimeString('yyyymmddhhnnsszzz', #0, #0) + '.old';
      if RenameFile(Targets[I], Aside) then
      begin
        SetArrayLength(MovedAside, GetArrayLength(MovedAside) + 2);
        MovedAside[GetArrayLength(MovedAside) - 2] := Targets[I];
        MovedAside[GetArrayLength(MovedAside) - 1] := Aside;
      end;
    end;
end;

// Splits a PATH value on semicolons and drops every entry naming Dir, keeping all other entries,
// empty ones included, exactly as they were.
function PathWithout(Path, Dir: String; var Found: Boolean): String;
var
  Rest, Entry: String;
  P: Integer;
  First: Boolean;
begin
  Result := '';
  Found := False;
  First := True;
  Rest := Path;
  repeat
    P := Pos(';', Rest);
    if P = 0 then
      Entry := Rest
    else
    begin
      Entry := Copy(Rest, 1, P - 1);
      Rest := Copy(Rest, P + 1, Length(Rest));
    end;
    if CompareText(RemoveBackslashUnlessRoot(Trim(Entry)), RemoveBackslashUnlessRoot(Dir)) = 0 then
      Found := True
    else
    begin
      if not First then Result := Result + ';';
      Result := Result + Entry;
      First := False;
    end;
  until P = 0;
end;

// Writes the user PATH. An existing value keeps its registry type (RegWriteStringValue preserves
// REG_EXPAND_SZ and leaves REG_SZ as REG_SZ), so %VARIABLE% entries the user already has are not
// expanded where they were not before; only a new value is created as REG_EXPAND_SZ.
function WriteUserPath(Value: String; Existed: Boolean): Boolean;
begin
  if Existed then
    Result := RegWriteStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Value)
  else
    Result := RegWriteExpandStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Value);
end;

// Adds or removes {app}\\${cli} in the user PATH (HKCU\\Environment), touching no other entry.
// ChangesEnvironment broadcasts WM_SETTINGCHANGE when Setup or Uninstall finishes.
procedure UpdateUserPath(Add: Boolean);
var
  Path, Remaining: String;
  Existed, Found: Boolean;
begin
  Existed := RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Path);
  if not Existed then Path := '';
  Remaining := PathWithout(Path, CliDir, Found);
  if Add then
  begin
    if Found then Exit;
    // A trailing separator stays trailing, so removing the entry restores the value byte for byte.
    if Path = '' then
      Path := CliDir
    else if Path[Length(Path)] = ';' then
      Path := Path + CliDir + ';'
    else
      Path := Path + ';' + CliDir;
    if not WriteUserPath(Path, Existed) then
      SuppressibleMsgBox('Could not add ' + CliDir + ' to the user PATH.', mbError, MB_OK, IDOK);
  end
  else if Found then
  begin
    if Remaining = '' then
      RegDeleteValue(HKEY_CURRENT_USER, 'Environment', 'Path')
    else if not WriteUserPath(Remaining, Existed) then
      Log('Could not remove ' + CliDir + ' from the user PATH.');
  end;
end;

// Reads the "version" string of current.json; the writers (this installer and vulcan-install-core)
// emit compact JSON whose version needs no escapes.
function CurrentVersion: String;
var
  Text: AnsiString;
  S: String;
  P: Integer;
begin
  Result := '';
  if not LoadStringFromFile(ExpandConstant('{app}\\${current}'), Text) then Exit;
  S := String(Text);
  P := Pos('"version"', S);
  if P = 0 then Exit;
  S := Trim(Copy(S, P + 9, Length(S)));
  if (S = '') or (S[1] <> ':') then Exit;
  S := Trim(Copy(S, 2, Length(S)));
  if (S = '') or (S[1] <> '"') then Exit;
  S := Copy(S, 2, Length(S));
  P := Pos('"', S);
  if P > 0 then Result := Copy(S, 1, P - 1);
end;

// Replaces current.json atomically: a complete partial file is renamed over the old one.
procedure SwitchCurrentVersion;
var
  Target: String;
begin
  Target := ExpandConstant('{app}\\${current}');
  if not SaveStringToFile(Target + '.partial', CurrentJson, False) then
    RaiseException('Could not write ' + Target + '.partial.');
  if not MoveFileEx(Target + '.partial', Target, MOVEFILE_REPLACE_EXISTING or MOVEFILE_WRITE_THROUGH) then
    RaiseException('Could not replace ' + Target + '.');
end;

// True when a program or library under Dir is loaded by a running process: opening a running image
// for writing fails even when every kind of sharing is allowed, and the open changes nothing.
// Windows lets a directory holding a running executable be renamed, so a rename cannot tell.
function ImagesInUse(Dir: String): Boolean;
var
  Search: TFindRec;
  Path, Ext: String;
  Stream: TFileStream;
begin
  Result := False;
  if not FindFirst(Dir + '\\*', Search) then Exit;
  try
    repeat
      if (Search.Name <> '.') and (Search.Name <> '..') then
      begin
        Path := Dir + '\\' + Search.Name;
        if Search.Attributes and FILE_ATTRIBUTE_DIRECTORY <> 0 then
        begin
          if Search.Attributes and FILE_ATTRIBUTE_REPARSE_POINT = 0 then Result := ImagesInUse(Path);
        end
        else
        begin
          Ext := LowerCase(ExtractFileExt(Search.Name));
          if (Ext = '.exe') or (Ext = '.dll') then
            try
              Stream := TFileStream.Create(Path, OPEN_WRITE_SHARE_ALL);
              Stream.Free;
            except
              Result := True;
            end;
        end;
      end;
    until Result or not FindNext(Search);
  finally
    FindClose(Search);
  end;
end;

// Keeps the installed version and the one it replaces. A version with a running program stays; each
// other one is renamed into updates\\ first, so the launcher never sees a half-deleted version.
procedure RemoveOlderVersions(Previous: String);
var
  Search: TFindRec;
  Versions, Updates, Name: String;
begin
  Versions := ExpandConstant('{app}\\${versions}');
  Updates := ExpandConstant('{app}\\${updates}');
  if not FindFirst(Versions + '\\*', Search) then Exit;
  try
    repeat
      Name := Search.Name;
      if (Name <> '.') and (Name <> '..') and (Search.Attributes and FILE_ATTRIBUTE_DIRECTORY <> 0)
        and (CompareText(Name, InstalledVersion) <> 0) and (CompareText(Name, Previous) <> 0) then
      begin
        if ImagesInUse(Versions + '\\' + Name) then
          Log('Version ' + Name + ' is in use and stays installed.')
        else
        begin
          ForceDirectories(Updates);
          if RenameFile(Versions + '\\' + Name, Updates + '\\' + Name + '.removing') then
            DelTree(Updates + '\\' + Name + '.removing', True, True, True)
          else
            Log('Version ' + Name + ' could not be moved and stays installed.');
        end;
      end;
    until not FindNext(Search);
  finally
    FindClose(Search);
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Previous: String;
begin
  if CurStep = ssPostInstall then
  begin
    Previous := CurrentVersion;
    // INSTALL RECORD HOOK: write versions\\<version>\\.vulcan-install.json here, after the files and
    // before current.json points at the version, in the format vulcan-install-core defines.
    SwitchCurrentVersion;
    Installed := True;
    // Repairing the current version does not know which version it replaced, so it keeps them all.
    if CompareText(Previous, InstalledVersion) <> 0 then RemoveOlderVersions(Previous);
    UpdateUserPath(WizardIsTaskSelected('addtopath'));
  end;
end;

procedure DeinitializeSetup;
var
  I: Integer;
begin
  if Installed then Exit;
  for I := 0 to GetArrayLength(MovedAside) div 2 - 1 do
    if not FileExists(MovedAside[I * 2]) then
      RenameFile(MovedAside[I * 2 + 1], MovedAside[I * 2]);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then UpdateUserPath(False);
end;
`;
}

// runInherited runs one tool with the console attached, so long compressions show their progress.
// runInherited 在附加控制台的情况下运行工具，使长时间压缩能显示进度。
function runInherited(executable, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(executable, args, { stdio: 'inherit', windowsHide: true });
    child.on('error', reject);
    child.on('exit', code => (code === 0 ? resolve() : reject(new Error(`${path.basename(executable)} exited with code ${code}.`))));
  });
}

// ensureInnoSetup installs the locked Inno Setup compiler into third_party and returns ISCC.exe.
// ensureInnoSetup 将锁定版本的 Inno Setup 编译器安装到 third_party，并返回 ISCC.exe。
async function ensureInnoSetup() {
  const lock = JSON.parse(await readFile(new URL('./build-inputs.lock.json', import.meta.url), 'utf8'));
  const { version, installer } = lock.inno_setup;
  const paths = buildInputPaths(repository);
  const home = path.join(paths.innoSetup, version);
  const compiler = path.join(home, 'ISCC.exe');
  try {
    await access(compiler);
    return compiler;
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  // The installer runs from its own directory with its expected name; the cache entry stays digest-named.
  // 安装器以预期名称在独立目录中运行；缓存条目仍按摘要命名。
  const download = path.join(paths.innoSetup, '.download');
  const partial = `${home}.partial`;
  await rm(download, { recursive: true, force: true });
  await rm(partial, { recursive: true, force: true });
  await mkdir(download, { recursive: true });
  try {
    const executable = path.join(download, installer.name);
    await writeFile(executable, await obtainLockedInput(installer, paths.lockedDownloads));
    console.log(`==> Installing Inno Setup ${version} into ${home}`);
    // Portable mode writes no uninstall entry; the current-user mode never asks for elevation.
    // 便携模式不写入卸载项；当前用户模式不请求提权。
    await runInherited(executable, ['/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CURRENTUSER', '/PORTABLE=1', `/DIR=${partial}`]);
    await access(path.join(partial, 'ISCC.exe'));
    await rename(partial, home);
  } finally {
    await rm(download, { recursive: true, force: true });
    await rm(partial, { recursive: true, force: true });
  }
  return compiler;
}

// packageWindowsInstaller verifies the release distribution, then compiles the setup program from it as is.
// packageWindowsInstaller 验证正式版发行物，然后按其现状编译安装程序。
async function packageWindowsInstaller(outputOverride) {
  if (process.platform !== 'win32' || process.arch !== 'x64') throw new Error('The Windows installer is built on Windows x64 only.');
  const { root, layout, version } = await inspectReleaseDistribution();
  const outputDir = outputOverride ?? path.join(repository, 'target', 'release-packages', version);
  await mkdir(outputDir, { recursive: true });
  // The launchers are copied out of the version tree, so a distribution built without them cannot be installed.
  // 启动器从版本目录中复制出来，因此缺少启动器的发行物无法安装。
  for (const { binary } of launcherSources) await access(path.join(root, 'bin', `${binary}.exe`));
  // The shortcuts take their identity from the same resolver the packaged desktop process uses.
  // 快捷方式的身份取自打包后桌面进程所用的同一解析器。
  const { resolveGuiNotificationIdentity } = await import(pathToFileURL(
    path.join(repository, 'apps', 'vulcan-code-desktop', 'dist', 'gui', 'notification-identity-values.js'),
  ).href);
  const compiler = await ensureInnoSetup();
  const script = innoSetupScript({
    root, layout, version, outputDir,
    entries: (await readdir(root)).sort(),
    icon: path.join(repository, 'apps', 'vulcan-code-service', 'icons', 'icon.ico'),
    identity: resolveGuiNotificationIdentity(path.join(root, layout.desktopExecutable), 'win32'),
  });
  // The generated script lives beside the setup program so a failed build can be inspected.
  // 生成的脚本与安装程序放在一起，便于检查失败的构建。
  const scriptPath = path.join(outputDir, `${installerAssetName(layout, version)}.iss`);
  await writeFile(scriptPath, script);
  console.log(`==> Compiling ${path.basename(scriptPath)} (unsigned)`);
  await runInherited(compiler, ['/Qp', scriptPath]);
  console.log(`Windows installer: ${path.join(outputDir, `${installerAssetName(layout, version)}.exe`)}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, ...rest] = process.argv.slice(2);
  if (command === 'package' && (rest.length === 0 || (rest.length === 2 && rest[0] === '--out'))) {
    await packageWindowsInstaller(rest[1] && path.resolve(rest[1]));
  } else {
    throw new Error('Usage: node scripts/windows-installer.mjs package [--out <dir>]');
  }
}
