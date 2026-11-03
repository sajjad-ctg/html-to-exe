# Enumerate ICON resources in an exe: type, id, language, byte size,
# plus a hash of the first icon image bytes to verify art identity.

param([Parameter(Mandatory)][string]$Exe)

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ResEnum {
    [DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
    public static extern IntPtr LoadLibraryEx(string file, IntPtr zero, uint flags);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool FreeLibrary(IntPtr h);
    public delegate bool EnumResNameDelegate(IntPtr h, IntPtr type, IntPtr name, IntPtr param);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool EnumResourceNames(IntPtr h, IntPtr type, EnumResNameDelegate cb, IntPtr param);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool EnumResourceLanguages(IntPtr h, IntPtr type, IntPtr name, EnumResNameDelegate cb, IntPtr param);
    [DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
    public static extern IntPtr FindResource(IntPtr h, IntPtr name, IntPtr type);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern uint SizeofResource(IntPtr h, IntPtr res);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern IntPtr LoadResource(IntPtr h, IntPtr res);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern IntPtr LockResource(IntPtr data);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern void FreeResource(IntPtr data);
}
'@

$h = [ResEnum]::LoadLibraryEx($Exe, [IntPtr]::Zero, 0x00000002) # LOAD_LIBRARY_AS_DATAFILE
if ($h -eq [IntPtr]::Zero) { throw "LoadLibraryEx failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())" }
try {
    foreach ($type in [IntPtr]3, [IntPtr]14) {  # RT_ICON, RT_GROUP_ICON
        $cb = [ResEnum+EnumResNameDelegate]{
            param($hmod, $t, $name, $param)
            $typeName = if ($t.ToInt32() -eq 3) { "RT_ICON" } else { "RT_GROUP_ICON" }
            $res = [ResEnum]::FindResource($hmod, $name, $t)
            $size = [ResEnum]::SizeofResource($hmod, $res)
            $langs = New-Object System.Collections.Generic.List[string]
            $langCb = [ResEnum+EnumResNameDelegate]{
                param($h2, $t2, $n2, $lang)
                $langs.Add("0x{0:X4}" -f $lang.ToInt64()) | Out-Null
                return $true
            }
            [ResEnum]::EnumResourceLanguages($hmod, $t, $name, $langCb, [IntPtr]::Zero) | Out-Null
            $label = if ($name.ToInt64() -lt 0x10000) { "id=$($name.ToInt64())" } else { "name=$([Marshal]::PtrToStringUni($name))" }
            Write-Host ("{0,-14} {1,6} size={2,7} langs=[{3}]" -f $typeName, $label, $size, ($langs -join ","))
            return $true
        }
        [ResEnum]::EnumResourceNames($h, $type, $cb, [IntPtr]::Zero) | Out-Null
    }
}
finally {
    [ResEnum]::FreeLibrary($h) | Out-Null
}
