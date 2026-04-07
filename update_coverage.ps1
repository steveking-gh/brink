Write-Host "Running cargo llvm-cov..."
$coverageOutput = & cargo llvm-cov --all-features --workspace 2>&1
$cleanOutput = ($coverageOutput | ForEach-Object { $_.ToString() }) -join "`n"

$readmePath = "README.md"
$readmeContent = Get-Content -Raw -Encoding UTF8 $readmePath

$startTag = "<!-- COVERAGE_START -->"
$endTag = "<!-- COVERAGE_END -->"

# Extracting only the table part from the output (starts at Filename, ends at TOTAL)
$tableLines = $cleanOutput -split "`n" | Where-Object { $_ -match "^Filename" -or $_ -match "^---" -or ($_ -match "^(.*)\s+\d+\s+\d+\s+\d+\.\d+%" -and -not $_.StartsWith("Uncovered")) }
if ($tableLines.count -gt 0) {
    # Keep only the summary table
    $tableStartIndex = for ($i = 0; $i -lt $tableLines.Length; $i++) { if ($tableLines[$i] -match "^Filename") { $i; break } }
    $tableEndIndex = for ($i = $tableStartIndex; $i -lt $tableLines.Length; $i++) { if ($tableLines[$i] -match "^TOTAL") { $i; break } }
    $tableText = $tableLines[$tableStartIndex..$tableEndIndex] -join "`n"
    
    $newContent = $startTag + "`n" + '```text' + "`n" + $tableText + "`n" + '```' + "`n" + $endTag
    
    $pattern = "(?s)<!-- COVERAGE_START -->.*?<!-- COVERAGE_END -->"
    $updatedReadme = $readmeContent -replace $pattern, $newContent
    
    Set-Content -Path $readmePath -Value $updatedReadme -Encoding UTF8
    Write-Host "Successfully updated README.md with the latest coverage report!"
} else {
    Write-Host "Failed to parse the coverage table." -ForegroundColor Red
}
