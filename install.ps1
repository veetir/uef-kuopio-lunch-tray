# LunchTray installer / updater for Windows.
#
#   irm https://raw.githubusercontent.com/veetir/uef-kuopio-lunch-tray/master/install.ps1 | iex

$ErrorActionPreference = 'Stop'

# Older machines still default to TLS 1.0, which GitHub refuses
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$repo = 'veetir/uef-kuopio-lunch-tray'
$dir  = "$env:LOCALAPPDATA\Programs\LunchTray"
$zip  = "$env:TEMP\LunchTray.zip"
$lnk  = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\LunchTray.lnk"

Write-Host 'Looking up the latest LunchTray release...'
$release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases?per_page=100" -UseBasicParsing |
    Where-Object { -not $_.draft -and -not $_.prerelease -and $_.tag_name -like 'windows-v*' } |
    Select-Object -First 1
if (-not $release) { throw "No published Windows release found in $repo." }

# Matches both LunchTray-windows-x64-v*.zip and the older
# compass-lunch-windows-x64-v*.zip, so downgrades and reinstalls still work.
$asset = $release.assets | Where-Object { $_.name -like '*windows-x64*.zip' } | Select-Object -First 1
if (-not $asset) { throw "Release $($release.tag_name) has no Windows zip asset." }

# 1.4.2 and earlier shipped compass-lunch.exe; 1.4.3 renamed it to LunchTray.exe.
# The zip is named after the binary it carries, so it tells us which to expect.
$exeName = if ($asset.name -like 'compass-lunch-*') { 'compass-lunch.exe' } else { 'LunchTray.exe' }

Write-Host "Installing $($release.tag_name) to $dir"

# Both names: installs from 1.4.2 and earlier ran as compass-lunch.exe.
Get-Process LunchTray, compass-lunch -ErrorAction SilentlyContinue |
    Stop-Process -Force -PassThru |
    Wait-Process -Timeout 10 -ErrorAction SilentlyContinue

New-Item $dir -ItemType Directory -Force | Out-Null
Invoke-WebRequest $asset.browser_download_url -OutFile $zip -UseBasicParsing
Expand-Archive $zip -DestinationPath $dir -Force
Remove-Item $zip -Force

$exe = "$dir\$exeName"
if (-not (Test-Path $exe)) { throw "The downloaded zip did not contain $exeName." }

# Leftovers from the pre-1.4.3 name, cleared only once the new binary is in
# place. Settings, favorites, themes and cache all live in
# %LOCALAPPDATA%\LunchTray and are migrated by the app itself, so nothing here
# touches user data
if ($exeName -ne 'compass-lunch.exe') {
    Remove-Item "$dir\compass-lunch.exe" -Force -ErrorAction SilentlyContinue
}
Remove-Item "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Compass Lunch.lnk" -Force -ErrorAction SilentlyContinue

# Clears any mark-of-the-web so SmartScreen does not prompt on launch.
Get-ChildItem $dir -Recurse -File | Unblock-File

$shortcut = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk)
$shortcut.TargetPath = $exe
$shortcut.WorkingDirectory = $dir
$shortcut.Description = 'UEF Kuopio campus lunch menus in the system tray'
$shortcut.Save()

Start-Process $exe
Write-Host "LunchTray $($release.tag_name) is installed and running in your system tray."
Write-Host 'Left-click the tray icon to open it. Right-click for settings.'
