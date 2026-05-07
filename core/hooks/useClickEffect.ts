import { ref } from 'vue'

export type ClickEffectType = 'explosion' | 'ripple' | 'halo'

// Emoji image file names
const EMOJI_NAMES = [
  'k0q','k3m','k3n','kyh','kyi','l6+','l6-','l7a','l7b','l7c','l7e','l7f','l7g','l7h','l7i','l7j','l7n',
  'l90','l9j','l9m','l=a','lea','leb','lec','led','lg9','lgr','lgs','lgt','lgu','lgv','lgw','lgx','lh2',
  'lh5','li2','li9','lin','lk9','lk=','llj','m0p','m3q','m3r','m3s','m3t','m3u','m3v','m66','m6z','m80',
  'm84','m85','m8d','m8e','m8f','m8g','m8h','m8v','m8w','m8x','m8y','m8z','mhp','mj0','mld','mq0','mq1',
  'ms3','m_v','nae','nai','naj','nan','p+u','p+z','pgd','pi+','pi_','pj-','pj2','pj3','pj4','pj5','pj6',
  'pj7','pj8','pj9','pja','pjb','pjd','pje','pjf','pjg','pji','pjk','pjl','pjm','pjp','pjq','pjr','pjt',
  'pju','pjv','pjw','pjx','pjy','pjz','pj_','pk+','pk-','pk1','pk4','pk5','pk6','pk8','pk9','pk=','pka',
  'pkb','pkc','pkd','pkf','pkg','pkh','pki','pkj','pkk','pkm','pkn','pko','pkq','pks','pkv','pk_','pl5',
  'pl6','pl8','pl9','pla','plb','pld','ple','plf','plh','pli','plj','pll','plm','plq','plr','pls','plt',
  'plu','plx','plz','pl_','pm0','pma','pmb','pmc','pmg','pmh','pmi','pmj','pml','pmn','pmo','pmp','pmq',
  'pmt','pmu','pmv','pmw','pmx','pmz','q-a','q78','q7=','q8b','qef','qf-','qf0','qf2','qf3','qf4','qf5',
  'qf7','qf9','qf=','qfz','qf_','qgj','qgy','qkn','qp4','qp9','qqn','q_+','r++','r+=','r+_','r7j','rei',
  'rns','ro0','ro3','ror','roy','roz','rpl','se0','sef','seg','sej','sil','sim','sin','sio','t+c','t+d',
  't+f','t+h','t+o','t+r','t+s','t-8','t-9','t=t','t=u','ucn','uco','ucq','ucs','ucw','ud3','ud5','udg',
  'udj','udo','udp','udr','uds'
]

const IMAGE_FOLDER = '/dy_emoji/'
const EXPLOSION_PARTICLES = 20

interface Particle {
  x: number
  y: number
  life: number
  update(): void
  draw(ctx: CanvasRenderingContext2D): void
}

// Image cache
const imageCache: Record<string, HTMLImageElement> = {}

function getRandomEmoji(): HTMLImageElement {
  const name = EMOJI_NAMES[Math.floor(Math.random() * EMOJI_NAMES.length)]
  if (imageCache[name!]) return imageCache[name!]!
  const img = new Image()
  img.src = `${IMAGE_FOLDER}${name}.webp`
  imageCache[name!] = img
  return img
}

// Particle Classes
class ImageExplosionParticle implements Particle {
  x: number
  y: number
  vx: number
  vy: number
  gravity: number
  friction: number
  life: number
  decay: number
  size: number
  image: HTMLImageElement
  rotation: number
  rotationSpeed: number

  constructor(x: number, y: number, baseDecay: number) {
    this.x = x
    this.y = y
    const angle = Math.random() * Math.PI * 2
    const speed = Math.random() * 5 + 3
    this.vx = Math.cos(angle) * speed
    this.vy = Math.sin(angle) * speed
    this.gravity = 0.15
    this.friction = 0.98
    this.life = 1.0
    this.decay = baseDecay * 1.2
    this.size = Math.random() * 30 + 20
    this.image = getRandomEmoji()
    this.rotation = Math.random() * Math.PI * 2
    this.rotationSpeed = (Math.random() - 0.5) * 0.2
  }

  update() {
    this.vx *= this.friction
    this.vy *= this.friction
    this.vy += this.gravity
    this.x += this.vx
    this.y += this.vy
    this.rotation += this.rotationSpeed
    this.life -= this.decay
  }

  draw(ctx: CanvasRenderingContext2D) {
    if (this.life <= 0 || !this.image.complete) return
    ctx.save()
    ctx.translate(this.x, this.y)
    ctx.rotate(this.rotation)
    ctx.globalAlpha = Math.max(0, this.life)
    ctx.drawImage(this.image, -this.size / 2, -this.size / 2, this.size, this.size)
    ctx.restore()
  }
}

class RippleParticle implements Particle {
  x: number
  y: number
  radius: number
  maxRadius: number
  life: number
  decay: number
  color: string
  lineWidth: number

  constructor(x: number, y: number, baseDecay: number) {
    this.x = x
    this.y = y
    this.radius = 5
    this.maxRadius = Math.random() * 100 + 150
    this.life = 1.0
    this.decay = baseDecay * 0.8
    const colors = ['#AEEEEE', '#E0FFFF', '#F0F8FF', '#87CEFA', '#409EFF', '#67C23A']
    this.color = colors[Math.floor(Math.random() * colors.length)]!
    this.lineWidth = Math.random() * 3 + 2
  }

  update() {
    this.radius += (this.maxRadius - this.radius) * 0.05
    this.life -= this.decay
  }

  draw(ctx: CanvasRenderingContext2D) {
    if (this.life <= 0) return
    ctx.save()
    ctx.beginPath()
    ctx.arc(this.x, this.y, this.radius, 0, Math.PI * 2)
    ctx.strokeStyle = this.color
    ctx.lineWidth = this.lineWidth
    ctx.globalAlpha = Math.max(0, this.life * (1 - this.radius / this.maxRadius))
    ctx.shadowBlur = 10
    ctx.shadowColor = this.color
    ctx.stroke()
    ctx.restore()
  }
}

class HaloParticle implements Particle {
  x: number
  y: number
  radius: number
  maxRadius: number
  life: number
  decay: number
  hue: number

  constructor(x: number, y: number, baseDecay: number) {
    this.x = x
    this.y = y
    this.radius = 10
    this.maxRadius = Math.random() * 150 + 250
    this.life = 1.0
    this.decay = baseDecay * 0.9
    this.hue = Math.random() * 60 + 30
  }

  update() {
    this.radius += (this.maxRadius - this.radius) * 0.03
    this.life -= this.decay
  }

  draw(ctx: CanvasRenderingContext2D) {
    if (this.life <= 0 || this.radius <= 0) return
    ctx.save()
    const gradient = ctx.createRadialGradient(this.x, this.y, 0, this.x, this.y, this.radius)
    const alpha = Math.max(0, this.life)
    gradient.addColorStop(0, `hsla(${this.hue}, 100%, 70%, ${alpha})`)
    gradient.addColorStop(0.4, `hsla(${this.hue}, 100%, 70%, ${alpha * 0.3})`)
    gradient.addColorStop(1, `hsla(${this.hue}, 100%, 70%, 0)`)
    ctx.beginPath()
    ctx.arc(this.x, this.y, this.radius, 0, Math.PI * 2)
    ctx.fillStyle = gradient
    ctx.globalCompositeOperation = 'lighter'
    ctx.fill()
    ctx.restore()
  }
}

export function useClickEffect() {
  const enabled = ref(false)
  const effectType = ref<ClickEffectType>('explosion')
  const duration = ref(400) // 150-700, default 400
  
  let canvas: HTMLCanvasElement | null = null
  let ctx: CanvasRenderingContext2D | null = null
  let particles: Particle[] = []
  let animationId: number | null = null
  let isRunning = false
  const pendingTimers = new Set<number>()

  const getBaseDecay = () => {
    return 0.015 * (100 / duration.value)
  }

  const createParticles = (x: number, y: number) => {
    if (!enabled.value) return
    
    const baseDecay = getBaseDecay()
    
    switch (effectType.value) {
      case 'explosion':
        for (let i = 0; i < EXPLOSION_PARTICLES; i++) {
          particles.push(new ImageExplosionParticle(x, y, baseDecay))
        }
        break
      case 'ripple':
        particles.push(new RippleParticle(x, y, baseDecay))
        if (Math.random() > 0.5) {
          const timerId = window.setTimeout(() => {
            pendingTimers.delete(timerId)
            if (enabled.value && isRunning) particles.push(new RippleParticle(x, y, baseDecay))
          }, 100)
          pendingTimers.add(timerId)
        }
        break
      case 'halo':
        particles.push(new HaloParticle(x, y, baseDecay))
        break
    }
  }

  const animate = () => {
    if (!ctx || !canvas) return
    
    // Clear with slight fade for trail effect
    ctx.fillStyle = 'rgba(0, 0, 0, 0.1)'
    ctx.globalCompositeOperation = 'destination-out'
    ctx.fillRect(0, 0, canvas.width, canvas.height)
    ctx.globalCompositeOperation = 'source-over'

    for (let i = particles.length - 1; i >= 0; i--) {
      const p = particles[i]!
      p.update()
      ctx.globalCompositeOperation = 'source-over'
      p.draw(ctx)
      if (p.life <= 0) {
        particles.splice(i, 1)
      }
    }

    if (isRunning) {
      animationId = requestAnimationFrame(animate)
    }
  }

  const handleClick = (e: PointerEvent) => {
    if (!enabled.value) return
    createParticles(e.clientX, e.clientY)
  }

  const resize = () => {
    if (canvas) {
      canvas.width = window.innerWidth
      canvas.height = window.innerHeight
    }
  }

  const init = () => {
    if (canvas) return // Already initialized
    
    canvas = document.createElement('canvas')
    canvas.id = 'click-effect-canvas'
    canvas.style.cssText = `
      position: fixed;
      top: 0;
      left: 0;
      width: 100%;
      height: 100%;
      pointer-events: none;
      z-index: 99999;
    `
    document.body.appendChild(canvas)
    ctx = canvas.getContext('2d')
    resize()
    
    window.addEventListener('resize', resize)
    window.addEventListener('pointerdown', handleClick)
    
    isRunning = true
    animate()
  }

  const destroy = () => {
    isRunning = false
    for (const id of pendingTimers) clearTimeout(id)
    pendingTimers.clear()
    if (animationId) {
      cancelAnimationFrame(animationId)
      animationId = null
    }
    window.removeEventListener('resize', resize)
    window.removeEventListener('pointerdown', handleClick)
    if (canvas && canvas.parentNode) {
      canvas.parentNode.removeChild(canvas)
    }
    canvas = null
    ctx = null
    particles = []
  }

  const setEnabled = (val: boolean) => {
    enabled.value = val
    if (val) {
      init()
    } else {
      destroy()
    }
  }

  const setEffectType = (type: ClickEffectType) => {
    effectType.value = type
  }

  const setDuration = (val: number) => {
    duration.value = val
  }

  // Load settings from localStorage
  const STORAGE_KEY = 'toolsbag-click-effect'
  
  const loadSettings = () => {
    try {
      const saved = localStorage.getItem(STORAGE_KEY)
      if (saved) {
        const data = JSON.parse(saved)
        effectType.value = data.effectType || 'explosion'
        duration.value = data.duration || 400
        if (data.enabled) {
          setEnabled(true)
        }
      }
    } catch {}
  }

  const saveSettings = () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({
      enabled: enabled.value,
      effectType: effectType.value,
      duration: duration.value
    }))
  }

  return {
    enabled,
    effectType,
    duration,
    setEnabled,
    setEffectType,
    setDuration,
    loadSettings,
    saveSettings,
    init,
    destroy
  }
}

// Singleton instance for global use
let instance: ReturnType<typeof useClickEffect> | null = null

export function getClickEffectInstance() {
  if (!instance) {
    instance = useClickEffect()
  }
  return instance
}
