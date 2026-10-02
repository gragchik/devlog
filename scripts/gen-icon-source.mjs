// Генерирует один одноцветный placeholder PNG (1024x1024) —
// исходник для `npx tauri icon`, который сам нарежет .ico/.icns/32x32/128x128
// и т.д. в src-tauri/icons/. Временное решение — заменить перед релизом
// (см. Итерация 10 в ТЗ).
import { mkdirSync, writeFileSync } from 'node:fs'
import { crc32, deflateSync } from 'node:zlib'

function chunk(type, data) {
  const typeBuf = Buffer.from(type, 'ascii')
  const len = Buffer.alloc(4)
  len.writeUInt32BE(data.length, 0)
  const crcBuf = Buffer.alloc(4)
  crcBuf.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0)
  return Buffer.concat([len, typeBuf, data, crcBuf])
}

function makePng(width, height, rgba) {
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])
  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(width, 0)
  ihdr.writeUInt32BE(height, 4)
  ihdr[8] = 8
  ihdr[9] = 6
  ihdr[10] = 0
  ihdr[11] = 0
  ihdr[12] = 0

  const stride = width * 4 + 1
  const raw = Buffer.alloc(stride * height)
  for (let y = 0; y < height; y++) {
    const rowStart = y * stride
    raw[rowStart] = 0
    for (let x = 0; x < width; x++) {
      const off = rowStart + 1 + x * 4
      raw[off] = rgba[0]
      raw[off + 1] = rgba[1]
      raw[off + 2] = rgba[2]
      raw[off + 3] = rgba[3]
    }
  }

  return Buffer.concat([signature, chunk('IHDR', ihdr), chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))])
}

mkdirSync('src-tauri/icons', { recursive: true })
const devlogBlue = [37, 99, 235, 255]
writeFileSync('src-tauri/icons/source.png', makePng(1024, 1024, devlogBlue))
console.log('Written src-tauri/icons/source.png — run: npx tauri icon src-tauri/icons/source.png')
