<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import * as echarts from 'echarts/core'
import { BarChart } from 'echarts/charts'
import { GridComponent, TooltipComponent } from 'echarts/components'
import { CanvasRenderer } from 'echarts/renderers'
import { useSettings } from '@core/hooks/useSettings'

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
    grid: { left: 20, right: 20, bottom: 30, top: 20, containLabel: true },
    xAxis: { type: 'category', data: props.durationBuckets.map((b) => b.label) },
    yAxis: { type: 'value' },
    series: [
      {
        name: t('数量'),
        type: 'bar',
        data: props.durationBuckets.map((b) => b.count),
        itemStyle: {
          borderRadius: [6, 6, 0, 0],
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

onMounted(renderChart)
watch(() => [props.durationBuckets, props.fileTypeTab], renderChart, { deep: true })
onBeforeUnmount(() => chartInstance?.dispose())
</script>

<template>
  <div class="stats-card">
    <div class="panel-header">
      <div>
        <h3>{{ props.fileTypeTab === 'video' ? t('视频分析数据') : t('图片分析数据') }}</h3>
      </div>
    </div>
    <div class="stats-grid">
      <div class="card-block basic-stats-block">
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
      <div class="card-block compact" v-if="props.fileTypeTab === 'video'">
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
      <div class="card-block compact" v-else>
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
</template>

<style scoped>
.stats-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 4px;
}
.stats-grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: 10px;
}
.card-block {
  padding: 12px;
  border: none;
  border-radius: 12px;
  background: transparent;
  box-shadow: none;
}
.card-block.compact {
  background: transparent;
}
.chart {
  width: 100%;
  height: 220px;
  min-height: 220px;
  background: linear-gradient(135deg, #f8f9ff, #eef2ff);
  border-radius: 10px;
  display: flex; /* Ensure chart container can hold canvas */
}
.card-block.basic-stats-block {
  background: transparent;
  border: none;
  box-shadow: none;
  padding: 0;
}
.block-title {
  margin: 0 0 10px;
  font-weight: 700;
}
.pill-vertical {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.pill-strip {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 14px;
  border-radius: 10px;
  background: linear-gradient(135deg, #eef2ff, #f8f9ff);
  border: 1px solid rgba(20, 23, 31, 0.04);
}
.pill-strip .label {
  color: #2b3040;
  font-size: 13px;
  margin: 0;
}
.pill-strip .value {
  margin: 0;
  font-weight: 700;
  font-size: 15px;
  color: #0f1527;
}
.pill-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: 10px;
}
.pill {
  padding: 10px 12px;
  border-radius: 10px;
  background: linear-gradient(135deg, #eef2ff, #f8f9ff);
  border: 1px solid rgba(20, 23, 31, 0.04);
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.pill .label {
  color: #2b3040;
  font-size: 12px;
  margin: 0;
}
.pill .value {
  margin: 0;
  font-weight: 700;
  font-size: 16px;
  color: #0f1527;
}
.stat-grid {
  margin-top: 6px;
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
}
.stat {
  padding: 10px;
  border-radius: 10px;
  background: #f6f8ff;
  border: 1px solid rgba(20, 23, 31, 0.04);
  min-height: 68px;
  display: flex;
  flex-direction: column;
  justify-content: center;
}
.stat .label {
  margin: 0;
  color: #2b3040;
  font-size: 12px;
}
.stat .value {
  margin: 2px 0 0;
  font-weight: 700;
  color: #0f1527;
}
.stat .value.small {
  font-weight: 600;
  font-size: 13px;
  line-height: 1.4;
}
.format-chart {
  margin-top: 6px;
}
.format-bars {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.format-row {
  display: grid;
  grid-template-columns: 60px 1fr 40px;
  gap: 8px;
  align-items: center;
}
.fmt-name {
  font-size: 13px;
  color: #4b5163;
}
.fmt-bar {
  height: 8px;
  background: #eef2ff;
  border-radius: 6px;
  overflow: hidden;
}
.fmt-fill {
  height: 100%;
  background: linear-gradient(135deg, #6f8cff, #9cb8ff);
}
.fmt-count {
  font-weight: 700;
  color: #2f73ff;
}
</style>
