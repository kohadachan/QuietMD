$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

Add-Type -AssemblyName System.Drawing

$assetDirectory = Split-Path -Parent $MyInvocation.MyCommand.Path
$sizes = @(16, 20, 24, 32, 40, 48, 64, 128, 256)
$pngImages = [System.Collections.Generic.List[byte[]]]::new()

function New-IconBitmap([int] $size) {
    $bitmap = [System.Drawing.Bitmap]::new(
        $size,
        $size,
        [System.Drawing.Imaging.PixelFormat]::Format32bppArgb
    )
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.Clear([System.Drawing.Color]::Transparent)
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $graphics.ScaleTransform($size / 256.0, $size / 256.0)

    $page = [System.Drawing.Drawing2D.GraphicsPath]::new()
    $page.StartFigure()
    $page.AddLine(60, 20, 152, 20)
    $page.AddLine(152, 20, 216, 84)
    $page.AddLine(216, 84, 216, 216)
    $page.AddBezier(216, 216, 216, 227, 207, 236, 196, 236)
    $page.AddLine(196, 236, 60, 236)
    $page.AddBezier(60, 236, 49, 236, 40, 227, 40, 216)
    $page.AddLine(40, 216, 40, 40)
    $page.AddBezier(40, 40, 40, 29, 49, 20, 60, 20)
    $page.CloseFigure()

    $fold = [System.Drawing.Drawing2D.GraphicsPath]::new()
    $fold.StartFigure()
    $fold.AddLine(152, 20, 216, 84)
    $fold.AddLine(216, 84, 176, 84)
    $fold.AddBezier(176, 84, 163, 84, 152, 73, 152, 60)
    $fold.CloseFigure()

    $m = [System.Drawing.Drawing2D.GraphicsPath]::new()
    $m.AddPolygon([System.Drawing.PointF[]] @(
        [System.Drawing.PointF]::new(62, 177),
        [System.Drawing.PointF]::new(62, 119),
        [System.Drawing.PointF]::new(76, 119),
        [System.Drawing.PointF]::new(92, 141),
        [System.Drawing.PointF]::new(108, 119),
        [System.Drawing.PointF]::new(122, 119),
        [System.Drawing.PointF]::new(122, 177),
        [System.Drawing.PointF]::new(106, 177),
        [System.Drawing.PointF]::new(106, 145),
        [System.Drawing.PointF]::new(92, 164),
        [System.Drawing.PointF]::new(78, 145),
        [System.Drawing.PointF]::new(78, 177)
    ))

    $d = [System.Drawing.Drawing2D.GraphicsPath]::new()
    $d.StartFigure()
    $d.AddLine(132, 119, 158, 119)
    $d.AddBezier(158, 119, 181, 119, 196, 130, 196, 148)
    $d.AddBezier(196, 148, 196, 166, 181, 177, 158, 177)
    $d.AddLine(158, 177, 132, 177)
    $d.CloseFigure()

    $dCutout = [System.Drawing.Drawing2D.GraphicsPath]::new()
    $dCutout.StartFigure()
    $dCutout.AddLine(149, 135, 158, 135)
    $dCutout.AddBezier(158, 135, 171, 135, 179, 139, 179, 148)
    $dCutout.AddBezier(179, 148, 179, 157, 171, 161, 158, 161)
    $dCutout.AddLine(158, 161, 149, 161)
    $dCutout.CloseFigure()

    $blue = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(255, 55, 120, 200))
    $foldBlue = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(255, 134, 180, 238))
    $white = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::White)

    $graphics.FillPath($blue, $page)
    $graphics.FillPath($foldBlue, $fold)
    $graphics.FillPath($white, $m)
    $graphics.FillPath($white, $d)
    $graphics.FillPath($blue, $dCutout)

    $blue.Dispose()
    $foldBlue.Dispose()
    $white.Dispose()
    $page.Dispose()
    $fold.Dispose()
    $m.Dispose()
    $d.Dispose()
    $dCutout.Dispose()
    $graphics.Dispose()
    return $bitmap
}

foreach ($size in $sizes) {
    $bitmap = New-IconBitmap $size
    $stream = [System.IO.MemoryStream]::new()
    $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
    $pngImages.Add($stream.ToArray())
    if ($size -eq 256) {
        $bitmap.Save(
            (Join-Path $assetDirectory "app-icon.png"),
            [System.Drawing.Imaging.ImageFormat]::Png
        )
    }
    $stream.Dispose()
    $bitmap.Dispose()
}

$iconPath = Join-Path $assetDirectory "app-icon.ico"
$iconStream = [System.IO.File]::Create($iconPath)
$writer = [System.IO.BinaryWriter]::new($iconStream)
$writer.Write([uint16] 0)
$writer.Write([uint16] 1)
$writer.Write([uint16] $sizes.Count)

$offset = 6 + 16 * $sizes.Count
for ($index = 0; $index -lt $sizes.Count; $index++) {
    $size = $sizes[$index]
    $image = $pngImages[$index]
    $writer.Write([byte] $(if ($size -eq 256) { 0 } else { $size }))
    $writer.Write([byte] $(if ($size -eq 256) { 0 } else { $size }))
    $writer.Write([byte] 0)
    $writer.Write([byte] 0)
    $writer.Write([uint16] 1)
    $writer.Write([uint16] 32)
    $writer.Write([uint32] $image.Length)
    $writer.Write([uint32] $offset)
    $offset += $image.Length
}

foreach ($image in $pngImages) {
    $writer.Write($image)
}

$writer.Dispose()
$iconStream.Dispose()

Write-Host "Generated: $iconPath"
Write-Host "Sizes: $($sizes -join ', ')"
