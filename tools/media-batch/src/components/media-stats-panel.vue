<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import * as echarts from 'echarts/core'
import { BarChart } from 'echarts/charts'
import { GridComponent, TooltipComponent } from 'echarts/components'
import { CanvasRenderer } from 'echarts/renderers'
import { useSettings } from '@core/hooks/useSettings'
import { ArrowDown } from '@element-plus/icons-vue'

echarts.use([BarChart, GridComponent, TooltipComponent, CanvasRenderer])

type Bucket = { label: string; min: number; max: number; count: number }
type Stats = Record<string, any>

type Props = {
  fileTypeTab: 'video' | 'image'
  basicStats: Stats
  advancedStats: Stats
  durationBuckets: Bucket[]
}

const props = defineProps<Props>()

const chartRef = ref<HTMLDivElement | null>(null)
let chartInstance: echarts.ECharts | null = null
const { t } = useSettings()

const collapsed = ref(true)

const toggleCollapse = () => {
  collapsed.value = !collapsed.value
  if (!collapsed.value) {
    nextTick(renderChart)
  }
}

const renderChart = () => {
  if (props.fileTypeTab !== 'video') {
    chartInstance?.clear()
    return
  }
  if (!chartRef.value) return
  if (!chartInstance) {
    chartInstance = echarts.init(chartRef.value)
  }
  chartInstance.setOption({
    tooltip: {
      trigger: 'item',
      formatter: ({ name, value }: any) => `${name}: ${value} ${t('个')}`
    },
    grid: { left: 20, right: 16, bottom: 26, top: 14, containLabel: true },
    xAxis: { type: 'category', data: props.durationBuckets.map((b) => b.label), axisLabel: { fontSize: 10 } },
    yAxis: { type: 'value', axisLabel: { fontSize: 10 } },
    series: [
      {
        name: t('数量'),
        type: 'bar',
        data: props.durationBuckets.map((b) => b.count),
        itemStyle: {
          borderRadius: [4, 4, 0, 0],
          color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
            { offset: 0, color: '#6f8cff' },
            { offset: 1, color: '#9cb8ff' }
          ])
        }
      }
    ]
  })
  chartInstance.resize()
}

onMounted(() => {
  if (!collapsed.value) renderChart()
})
watch(() => [props.durationBuckets, props.fileTypeTab], () => {
  if (!collapsed.value) renderChart()
})
onBeforeUnmount(() => chartInstance?.dispose())
</script>

<template>
  <div class="stats-card">
    <div class="collapse-header" @click="toggleCollapse">
      <span class="collapse-label">{{ t('分析数据') }}</span>
      <span class="collapse-badge" v-if="collapsed">{{ t('展开查看') }}</span>
      <el-icon :size="13" :class="['collapse-icon', { 'is-open': !collapsed }]">
        <ArrowDown />
      </el-icon>
    </div>
    <div v-show="!collapsed" class="collapse-body">
      <div class="stats-grid">
        <div class="card-block">
          <div class="pill-vertical">
            <div class="pill-strip">
              <span class="label">{{ props.fileTypeTab === 'video' ? t('导入视频') : t('总图片') }}</span>
              <span class="value">{{ props.fileTypeTab === 'video' ? props.basicStats.totalVideos : props.advancedStats.totalImages }}</span>
            </div>
            <div v-if="props.fileTypeTab === 'video'" class="pill-strip">
              <span class="label">{{ t('总时长') }}</span>
              <span class="value">{{ props.basicStats.totalDuration }}</span>
            </div>
            <div class="pill-strip">
              <span class="label">{{ t('合计大小') }}</span>
              <span class="value">{{ props.basicStats.totalSize }}</span>
            </div>
          </div>
        </div>
        <div class="card-block" v-if="props.fileTypeTab === 'video'">
          <p class="block-title">{{ t('时长分布') }}</p>
          <div ref="chartRef" class="chart"></div>
        </div>
        <div class="card-block" v-else>
          <p class="block-title">{{ t('格式分布') }}</p>
          <div class="format-chart">
            <div class="format-bars">
              <div v-for="fmt in props.advancedStats.formatCounts || []" :key="fmt.ext" class="format-row">
                <span class="fmt-name">{{ fmt.ext }}</span>
                <div class="fmt-bar">
                  <div
                    class="fmt-fill"
                    :style="{
                      width:
                        props.advancedStats.formatCounts && props.advancedStats.formatCounts.length
                          ? (fmt.count / Math.max(...props.advancedStats.formatCounts.map((f: any) => f.count || 1)) * 100) + '%'
                          : '0%'
                    }"
                  ></div>
                </div>
                <span class="fmt-count">{{ fmt.count }}</span>
              </div>
            </div>
          </div>
        </div>
        <div class="card-block" v-if="props.fileTypeTab === 'video'">
          <p class="block-title">{{ t('极值 / 均值') }}</p>
          <div class="stat-grid">
            <div class="stat">
              <p class="label">{{ t('最长视频') }}</p>
              <p class="value small">{{ props.advancedStats.longest }}</p>
            </div>
            <div class="stat">
              <p class="label">{{ t('最短视频') }}</p>
              <p class="value small">{{ props.advancedStats.shortest }}</p>
            </div>
            <div class="stat">
              <p class="label">{{ t('最高码率') }}</p>
              <p class="value small">{{ props.advancedStats.maxBitrate }}</p>
            </div>
            <div class="stat">
              <p class="label">{{ t('最低码率') }}</p>
              <p class="value small">{{ props.advancedStats.minBitrate }}</p>
            </div>
            <div class="stat">
              <p class="label">{{ t('平均时长') }}</p>
              <p class="value small">{{ props.advancedStats.avgDuration }}</p>
            </div>
            <div class="stat">
              <p class="label">{{ t('平均码率') }}</p>
              <p class="value small">{{ props.advancedStats.avgBitrate }}</p>
            </div>
          </div>
        </div>
        <div class="card-block" v-else>
          <p class="block-title">{{ t('分辨率概览') }}</p>
          <div class="stat-grid">
            <div class="stat">
              <p class="label">{{ t('最大分辨率') }}</p>
              <p class="value small">{{ props.advancedStats.maxResolution }}</p>
            </div>
            <div class="stat">
              <p class="label">{{ t('最小分辨率') }}</p>
              <p class="value small">{{ props.advancedStats.minResolution }}</p>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.stats-card {
  display: flex;
  flex-direction: column;
  margin-top: 6px;
  border-radius: var(--card-radius);
  border: 1px solid var(--card-border);
  box-shadow: var(--card-shadow);
  background: var(--color-card);
}
.collapse-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 7px 10px;
  cursor: pointer;
  border-radius: var(--card-radius);
  transition: background var(--duration-fast) var(--ease-out);
  user-select: none;
}
.collapse-header:hover {
  background: var(--accent-light);
}
.collapse-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary);
  flex: 1;
}
.collapse-badge {
  font-size: 10px;
  color: var(--text-muted);
}
.collapse-icon {
  color: var(--text-muted);
  transition: transform var(--duration-fast) var(--ease-out);
}
.collapse-icon.is-open {
  transform: rotate(180deg);
}
.collapse-body {
  padding: 0 8px 8px;
}
.stats-grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: 6px;
}
.card-block {
  padding: 0;
  border: none;
}
.block-title {
  margin: 4px 0 4px;
  font-weight: 600;
  font-size: 11px;
  color: var(--text-secondary);
}
.chart {
  width: 100%;
  height: 120px;
  min-height: 120px;
  background: var(--bg-tertiary);
  border-radius: var(--radius-sm);
}
.pill-vertical {
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.pill-strip {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 4px 8px;
  border-radius: 6px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-secondary);
}
.pill-strip .label {
  color: var(--text-secondary);
  font-size: 11px;
  margin: 0;
}
.pill-strip .value {
  margin: 0;
  font-weight: 700;
  font-size: 12px;
  color: var(--text-primary);
}
.stat-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 4px;
}
.stat {
  padding: 4px 6px;
  border-radius: 5px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-secondary);
  display: flex;
  flex-direction: column;
  justify-content: center;
}
.stat .label {
  margin: 0;
  color: var(--text-muted);
  font-size: 10px;
}
.stat .value {
  margin: 1px 0 0;
  font-weight: 700;
  color: var(--text-primary);
}
.stat .value.small {
  font-weight: 600;
  font-size: 11px;
  line-height: 1.3;
}
.format-chart {
  margin-top: 2px;
}
.format-bars {
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.format-row {
  display: grid;
  grid-template-columns: 40px 1fr 28px;
  gap: 4px;
  align-items: center;
}
.fmt-name {
  font-size: 11px;
  color: var(--text-secondary);
}
.fmt-bar {
  height: 6px;
  background: var(--bg-tertiary);
  border-radius: 3px;
  overflow: hidden;
}
.fmt-fill {
  height: 100%;
  background: var(--accent);
}
.fmt-count {
  font-weight: 700;
  color: var(--accent);
  font-size: 11px;
}
</style>
