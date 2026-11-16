; Instalador de EzAgent (versión Rust). Compilar con:
;   ISCC.exe packaging\EzAgent-rust.iss
; o, mejor, con packaging\build-rust.ps1, que antes arma dist\rust-stage\.
;
; Se instala en LocalAppData para no requerir administrador. No instala
; Python. Los datos del usuario (%APPDATA%\EzAgent y %LOCALAPPDATA%\EzAgent\models,
; whisper, espeak-ng) están fuera de {app} y no se tocan al instalar ni al
; desinstalar: los modelos se descargan solos la primera vez.
;
; DECISION: mismo AppId que packaging\EzAgent.iss: para el usuario es el mismo
; producto y así la versión Rust reemplaza a la de Python en una actualización.
; La ruta de Python sigue disponible con EzAgent.bat py.

#define MyAppName "EzAgent"
#define MyAppVersion "0.2.0"
#define MyAppExeName "ezagent-voice.exe"

[Setup]
AppId={{A7C4C7B8-0C7E-4E8B-9A93-6B7A5D1F2E11}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher=EzAgent contributors
DefaultDirName={localappdata}\Programs\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=EzAgent-Setup-{#MyAppVersion}
SetupIconFile=icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
; El .exe lleva adentro el icono y la versión; el instalador no depende de eso.

[Languages]
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[Tasks]
Name: "desktopicon"; Description: "Crear un acceso directo en el escritorio"; GroupDescription: "Accesos directos:"; Flags: unchecked

[Files]
Source: "..\dist\rust-stage\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\Desinstalar {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Parameters: "downloads --all"; Description: "Descargar whisper y los modelos de voz (requiere Internet)"; StatusMsg: "Descargando whisper y los modelos de voz..."; Flags: postinstall waituntilterminated skipifsilent
Filename: "{app}\{#MyAppExeName}"; Description: "Abrir {#MyAppName}"; Flags: nowait postinstall skipifsilent
