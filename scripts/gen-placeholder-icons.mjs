// Генерирует простые одноцветные placeholder-иконки (resources/icon.png, resources/tray.png).
// Временное решение для Итерации 0 — заменить на реальный дизайн перед релизом (см. Итерация 10).
import { mkdirSync, writeFileSync } from 'node:fs'
import { deflateSync, crc32 } from 'node:zlib'

/** @param {string} type @param {Buffer} data */
function chunk(type, data) {
  const typeBuf = Buffer.from(type, 'ascii')
  const len = Buffer.alloc(4)
  len.writeUInt32BE(data.length, 0)
  const crcBuf = Buffer.alloc(4)
  crcBuf.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0)
  return Buffer.concat([len, typeBuf, data, crcBuf])
}

/** @param {number} width @param {number} height @param {[number, number, number, number]} rgba */
function makePng(width, height, rgba) {
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])
  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(width, 0)
  ihdr.writeUInt32BE(height, 4)
  ihdr[8] = 8 // bit depth
  ihdr[9] = 6 // color type RGBA
  ihdr[10] = 0 // compression
  ihdr[11] = 0 // filter
  ihdr[12] = 0 // interlace

  const stride = width * 4 + 1
  const raw = Buffer.alloc(stride * height)
  for (let y = 0; y < height; y++) {
    const rowStart = y * stride
    raw[rowStart] = 0 // filter: none
    for (let x = 0; x < width; x++) {
      const off = rowStart + 1 + x * 4
      raw[off] = rgba[0]
      raw[off + 1] = rgba[1]
      raw[off + 2] = rgba[2]
      raw[off + 3] = rgba[3]
    }
  }

  return Buffer.concat([
    signature,
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw)),
    chunk('IEND', Buffer.alloc(0))
  ])
}

mkdirSync('resources', { recursive: true })
const devlogBlue = /** @type {[number, number, number, number]} */ ([37, 99, 235, 255])
writeFileSync('resources/icon.png', makePng(256, 256, devlogBlue))
writeFileSync('resources/tray.png', makePng(32, 32, devlogBlue))
console.log('Placeholder icons written to resources/icon.png and resources/tray.png')
