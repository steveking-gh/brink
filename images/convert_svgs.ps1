# Verified for Inkscape 1.4.3 (2026)
$inkscapePath = "C:\Program Files\Inkscape\bin\inkscape.com"
$suffix = "_plain_no_text"

# 1. Gather only original SVG files
$files = Get-ChildItem -Filter "*.svg" | Where-Object { $_.Name -notlike "*$suffix.svg" }

if ($files.Count -eq 0) {
    Write-Host "No original SVG files found to process." -ForegroundColor Yellow
    exit
}

Write-Host "Processing $($files.Count) files..." -ForegroundColor Cyan

foreach ($file in $files) {
    $outputName = "$($file.BaseName)$($suffix).svg"
    $outputPath = ".\$outputName"

    # Skip conversion if the output already exists and is newer than the source
    if (Test-Path $outputPath) {
        if ((Get-Item $outputPath).LastWriteTime -gt $file.LastWriteTime) {
            Write-Host "Skipping: $($file.Name) (Already up to date)" -ForegroundColor Gray
            continue
        }
    }

    Write-Host "Converting: $($file.Name) -> $outputName... " -NoNewline

    # 2. The winning 1.4.3 command syntax
    & $inkscapePath "$($file.FullName)" `
        --export-text-to-path `
        --export-plain-svg `
        -o "$outputPath" 2>$null

    # 3. Final verification
    if (Test-Path $outputPath) {
        Write-Host "SUCCESS" -ForegroundColor Green
    } else {
        Write-Host "FAILED" -ForegroundColor Red
    }
}

Write-Host "`nAll diagrams processed!" -ForegroundColor Cyan