import { test, expect } from '@playwright/test';
test('常卖机型题库可完成客观题和评论情景题',async({page},testInfo)=>{
  const errors:string[]=[];
  page.on('pageerror',e=>errors.push(e.message));
  page.on('console',m=>{if(m.type()==='error')errors.push(m.text());});
  await page.setViewportSize({width:320,height:800});
  await page.addInitScript(()=>{(window as any).__TAURI_INTERNALS__={invoke:async(command:string,args:any)=>{if(command!=='minimax_training_json'||args?.schema!=='feedback')throw new Error('wrong training contract');return JSON.stringify({score:88,verdict:'部分正确',correctPoints:['先问了用途'],incorrectPoints:[],missingPoints:['没有确认预算'],keyImprovement:'补问预算。',liveImpact:'能减少推荐偏差。',reference:'请问您主要拍什么、预算多少，再按当前商品信息介绍。',followUp:'顾客预算不明确时还应该怎么问？',evidenceQuote:'佳能 EOS R6 Mark II'});}};});
  await page.goto('/tests/fixtures/coach-practice-harness.html',{waitUntil:'domcontentloaded'});
  await page.getByLabel('基础题型').selectOption('choice');
  await page.getByRole('button',{name:'常卖机型训练',exact:true}).click();
  await expect(page.getByText(/第 1\/12 题/)).toBeVisible();
  await page.getByRole('radio').first().check();
  await page.getByRole('button',{name:'提交回答'}).click();
  await expect(page.getByText(/正确答案：/)).toBeVisible();
  await page.getByLabel('基础题型').selectOption('open');
  await page.getByRole('button',{name:'常卖机型训练',exact:true}).click();
  await expect(page.getByText(/第 1\/36 题/)).toBeVisible();
  await page.getByLabel('你的现场回答').fill('先问主要拍照片还是视频。');
  await page.getByRole('button',{name:'提交回答'}).click();
  await expect(page.getByText('教练反馈 · 88 分')).toBeVisible();
  await expect(page.getByRole('heading',{name:'事实核对'})).toBeVisible();
  await expect(page.getByText('补问预算。')).toBeVisible();
  await expect(page.getByText('能减少推荐偏差。')).toBeVisible();
  await expect(page.getByText('顾客预算不明确时还应该怎么问？')).toBeVisible();
  await page.getByLabel('本题精准分析').screenshot({path:testInfo.outputPath('precise-analysis-320.png')});
  await page.getByRole('button',{name:'评论情景训练',exact:true}).click();
  await expect(page.getByText(/第 1\/24 题/)).toBeVisible();
  await expect(page.getByText(/X-S20和X-T30三代推荐哪个/)).toBeVisible();
  await page.screenshot({path:testInfo.outputPath('popular-camera-training-320.png'),fullPage:true});
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
  expect(errors).toEqual([]);
});
for (const kind of ['choice','boolean','mixed']) test(`客观题闭环 ${kind}`,async({page})=>{
  const errors:string[]=[];
  page.on('pageerror',e=>errors.push(e.message));
  page.on('console',m=>{if(m.type()==='error')errors.push(m.text());});
  await page.setViewportSize({width:320,height:800});
  await page.addInitScript(()=>{(window as any).__TAURI_INTERNALS__={invoke:async(c:string,args:any)=>{
    if(c==='get_coach_knowledge_cards')return [1,2,3].map(i=>({cardId:`K${i}`,title:'卡口',body:'机身与镜头卡口需要匹配。',relativePath:'卡口.md'}));
    if(c!=='minimax_training_json')throw new Error('wrong training command');
    if(args.schema==='question'&&args.systemPrompt.includes('基础知识出题教练'))return JSON.stringify({question:'机身与镜头卡口需要匹配吗？',options:args.systemPrompt.includes('四选一')?['需要','不需要','只看颜色','只看重量']:['正确','错误'],correctAnswer:'A',quote:'卡口需要匹配',explanation:'机身与镜头卡口需要匹配。'});
    if(args.schema!=='feedback')throw new Error('wrong training schema');
    return JSON.stringify({score:65,verdict:'部分正确',correctPoints:['提到了卡口'],incorrectPoints:[],missingPoints:['没有说完整型号'],keyImprovement:'补充确认完整型号。',liveImpact:'避免推荐无法安装的镜头。',reference:'先确认机身和镜头完整型号，再检查卡口。',followUp:'同品牌镜头一定能直接安装吗？',evidenceQuote:'卡口需要匹配'});
  }};});
  await page.goto('/tests/fixtures/coach-practice-harness.html',{waitUntil:'domcontentloaded'});
  await page.getByLabel('基础题型').selectOption(kind);
  await page.getByRole('button',{name:'基础知识问答',exact:true}).click();
  await expect(page.getByText(/正确答案：/)).toHaveCount(0);
  await page.getByRole('radio').first().focus();
  await page.keyboard.press('Space');
  await page.getByRole('button',{name:'提交回答'}).click();
  await expect(page.getByText('教练反馈 · 100 分')).toBeVisible();
  await expect(page.getByText(/正确答案：A/)).toBeVisible();
  await page.getByRole('button',{name:'下一题'}).click();
  await page.getByRole('radio').nth(1).check();
  await page.getByRole('button',{name:'提交回答'}).click();
  await expect(page.getByText('教练反馈 · 0 分')).toBeVisible();
  if(kind==='mixed'){
    await page.getByRole('button',{name:'下一题'}).click();
    await page.getByLabel('你的现场回答').fill('检查机身与镜头卡口');
    await page.getByRole('button',{name:'提交回答'}).click();
    await expect(page.getByText('教练反馈 · 65 分')).toBeVisible();
  }
  await page.getByRole('button',{name:'结束并总结'}).click();
  await expect(page.getByText('本次训练总结')).toBeVisible();
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
  expect(errors).toEqual([]);
});
for (const width of [320,768,1440]) test(`两类跟练与主播隔离 ${width}`,async({page},testInfo)=>{
  const errors:string[]=[];
  page.on('pageerror',e=>errors.push(e.message));
  page.on('console',m=>{if(m.type()==='error')errors.push(m.text());});
  await page.setViewportSize({width,height:800});
  await page.emulateMedia({reducedMotion:'reduce'});
  await page.addInitScript(()=>{
    (window as any).__TAURI_INTERNALS__={invoke:async(command:string,args:any)=>{
      if(command==='get_coach_knowledge_cards')return [{cardId:'K1',title:'卡口匹配',body:'购买前确认机身与镜头卡口匹配。',relativePath:'基础/卡口.md'}];
      if(command==='minimax_training_json' && args?.schema==='feedback')return JSON.stringify({score:65,verdict:'部分正确',correctPoints:['提到需要确认'],incorrectPoints:[],missingPoints:['没有说明依据'],keyImprovement:'说出核对依据。',liveImpact:'避免对二手商品作无证据承诺。',reference:'请先核实，以检测结果和页面为准。',followUp:'如果资料没有覆盖，应怎样向顾客说明？',evidenceQuote:String(args?.systemPrompt||'').includes('全网最低')?'全网最低':'购买前确认机身与镜头卡口匹配。'});
      throw Error(command);
    }};
  });
  await page.goto('/tests/fixtures/coach-practice-harness.html',{waitUntil:'domcontentloaded'});
  await page.getByRole('button',{name:'基础知识问答',exact:true}).click();
  await expect(page.getByText(/请向一位新手观众/)).toBeVisible();
  await expect(page.getByText('购买前确认机身与镜头卡口匹配。',{exact:true})).toHaveCount(0);
  await page.getByLabel('你的现场回答').fill('确认卡口');
  await page.getByRole('button',{name:'提交回答'}).focus();
  await page.keyboard.press('Enter');
  await expect(page.getByText('教练反馈 · 65 分')).toBeVisible();
  await page.getByText('查看本次判定依据').click();
  await expect(page.getByText('基础/卡口.md')).toBeVisible();
  await page.getByRole('button',{name:'查看总结'}).click();
  await expect(page.getByText('本次训练总结')).toBeVisible();
  await page.getByRole('button',{name:'直播敏感词训练',exact:true}).click();
  await page.getByLabel('你的现场回答').fill('不能说全网最低，需要确认。');
  await page.getByRole('button',{name:'提交回答'}).click();
  await expect(page.getByText('教练反馈 · 65 分')).toBeVisible();
  await page.getByRole('button',{name:'下一题'}).click();
  await expect(page.getByText(/第 2\/8 题/)).toBeVisible();
  await page.getByRole('button',{name:'结束并总结'}).click();
  await page.screenshot({path:testInfo.outputPath(`practice-${width}.png`),fullPage:true});
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
  await page.getByRole('button',{name:'切换主播'}).click();
  await expect(page.getByText('基础跟练 · 于千惠')).toBeVisible();
  await expect(page.getByText(/本主播历史训练/)).toHaveCount(0);
  expect(errors).toEqual([]);
});
test('空知识库与模型错误不伪造分数',async({page})=>{
  await page.addInitScript(()=>{(window as any).__TAURI_INTERNALS__={invoke:async(c:string)=>c==='get_coach_knowledge_cards'?[]:'{}'};});
  await page.goto('/tests/fixtures/coach-practice-harness.html',{waitUntil:'domcontentloaded'});
  await page.getByRole('button',{name:'基础知识问答',exact:true}).click();
  await expect(page.getByRole('alert')).toContainText('暂无可用知识卡');
  await page.getByRole('button',{name:'直播敏感词训练',exact:true}).click();
  await page.getByLabel('你的现场回答').fill('现场回答');
  await page.getByRole('button',{name:'提交回答'}).click();
  await expect(page.getByRole('alert')).toContainText('本次未计分');
  await expect(page.getByLabel('你的现场回答')).toHaveValue('现场回答');
});

test('零基础认机可完成学习、键盘查看和看图定位',async({page},testInfo)=>{
  const errors:string[]=[];
  page.on('pageerror',e=>errors.push(e.message));
  page.on('console',m=>{if(m.type()==='error')errors.push(m.text());});
  await page.setViewportSize({width:320,height:800});
  await page.emulateMedia({reducedMotion:'reduce'});
  await page.goto('/tests/fixtures/coach-practice-harness.html',{waitUntil:'domcontentloaded'});
  await page.getByRole('button',{name:'零基础认机',exact:true}).click();
  await expect(page.getByRole('heading',{name:'佳能 EOS R50'})).toBeVisible();
  await expect(page.getByText('看图认识 · 1/12')).toBeVisible();
  await expect(page.getByRole('img',{name:/佳能 EOS R50 参数卡原图/})).toBeVisible();
  await expect(page.getByText('有无闪光灯')).toBeVisible();
  await expect(page.getByRole('heading',{name:'主播先记住'})).toBeVisible();
  await expect(page.getByRole('heading',{name:'顾客常见痛点'})).toBeVisible();
  await expect(page.getByRole('heading',{name:'可以这样讲'})).toBeVisible();
  await expect(page.getByRole('heading',{name:'使用边界'})).toBeVisible();
  await page.getByRole('heading',{name:'主播先记住'}).scrollIntoViewIfNeeded();
  await expect(page.getByRole('heading',{name:'主播先记住'})).toBeInViewport();
  await page.screenshot({path:testInfo.outputPath('camera-knowledge-card-320.png'),fullPage:true});
  await page.getByRole('button',{name:'开始找快门按钮'}).click();
  await expect(page.getByText('请在相机图片上点出快门按钮')).toBeVisible();
  const camera = page.getByRole('button',{name:'在相机图片上选择快门按钮'});
  const box = await camera.boundingBox();
  expect(box).not.toBeNull();
  await camera.click({position:{x:box!.width*0.325,y:box!.height*0.385}});
  await expect(page.getByText('找对了')).toBeVisible();
  await page.getByRole('button',{name:'再找一次'}).click();
  await page.getByRole('button',{name:'显示快门按钮位置'}).focus();
  await page.keyboard.press('Enter');
  await expect(page.getByText('快门按钮位置已标出')).toBeVisible();
  await expect(page.getByRole('link',{name:'查看佳能官方部件图'})).toHaveAttribute('href',/cam\.start\.canon/);
  await page.screenshot({path:testInfo.outputPath('beginner-camera-320.png'),fullPage:true});
  for (const sample of [
    {name:'佳能 EOS R6 Mark II',x:.295,y:.44,file:'canon-r6ii-hotspot-320.png'},
    {name:'富士 X-T5',x:.315,y:.315,file:'fujifilm-xt5-hotspot-320.png'},
    {name:'索尼 α7 III',x:.225,y:.33,file:'sony-a7m3-hotspot-320.png'},
  ]) {
    await page.getByRole('button',{name:'学习下一款'}).click();
    await expect(page.getByRole('heading',{name:sample.name})).toBeVisible();
    await page.getByRole('button',{name:'开始找快门按钮'}).click();
    const nextCamera = page.getByRole('button',{name:'在相机图片上选择快门按钮'});
    const nextBox = await nextCamera.boundingBox();
    expect(nextBox).not.toBeNull();
    await nextCamera.click({position:{x:nextBox!.width*sample.x,y:nextBox!.height*sample.y}});
    await expect(page.getByText('找对了')).toBeVisible();
    await page.screenshot({path:testInfo.outputPath(sample.file),fullPage:true});
  }
  expect(await page.locator('#app').evaluate((element)=>element.scrollWidth<=element.clientWidth)).toBe(true);
  expect(errors).toEqual([]);
});

for (const viewport of [
  { width: 768, height: 900, name: 'tablet' },
  { width: 1440, height: 900, name: 'desktop' },
]) {
  test(`零基础资料卡在 ${viewport.width}px 可读且无横向溢出`, async ({ page }, testInfo) => {
    const errors:string[]=[];
    page.on('pageerror',e=>errors.push(e.message));
    page.on('console',m=>{if(m.type()==='error')errors.push(m.text());});
    await page.setViewportSize({width:viewport.width,height:viewport.height});
    await page.goto('/tests/fixtures/coach-practice-harness.html',{waitUntil:'domcontentloaded'});
    await page.getByRole('button',{name:'零基础认机',exact:true}).click();
    await expect(page.getByRole('heading',{name:'佳能 EOS R50'})).toBeVisible();
    await expect(page.getByRole('heading',{name:'顾客常见痛点'})).toBeVisible();
    await expect(page.getByRole('heading',{name:'卖点怎么理解'})).toBeVisible();
    await expect(page.getByRole('heading',{name:'使用边界'})).toBeVisible();
    expect(await page.locator('#app').evaluate((element)=>element.scrollWidth<=element.clientWidth)).toBe(true);
    await page.screenshot({path:testInfo.outputPath(`camera-knowledge-card-${viewport.name}.png`),fullPage:true});
    expect(errors).toEqual([]);
  });
}

test('公司基础课显示图片并完成三类题目入口',async({page},testInfo)=>{
  const errors:string[]=[];
  page.on('pageerror',e=>errors.push(e.message));
  page.on('console',m=>{if(m.type()==='error')errors.push(m.text());});
  await page.setViewportSize({width:320,height:800});
  await page.addInitScript(()=>{(window as any).__TAURI_INTERNALS__={invoke:async()=>JSON.stringify({score:96,verdict:'正确',correctPoints:['名称正确'],incorrectPoints:[],missingPoints:[],keyImprovement:'保持口语简洁。',liveImpact:'帮助新手快速理解。',reference:'这类相机叫微单，也叫无反相机。',followUp:'微单和单反结构上有什么主要区别？',evidenceQuote:'微单相机也称无反相机'} )};});
  await page.goto('/tests/fixtures/coach-practice-harness.html',{waitUntil:'domcontentloaded'});
  await page.getByRole('button',{name:'公司基础课',exact:true}).click();
  await expect(page.getByText(/公司基础课 · 第 1\/12 题/)).toBeVisible();
  const image = page.getByRole('img',{name:/公司摄影基础课程地图/});
  await expect(image).toBeVisible();
  await expect(image).toHaveAttribute('src',/training\/company-foundation/);
  await page.getByRole('radio').first().check();
  await page.getByRole('button',{name:'提交回答'}).click();
  await expect(page.getByText(/正确答案：/)).toBeVisible();
  await page.screenshot({path:testInfo.outputPath('company-foundation-320.png'),fullPage:true});
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
  expect(errors).toEqual([]);
});
