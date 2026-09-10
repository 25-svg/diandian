import assert from 'node:assert/strict';
import { knowledgeQuestions, parseFeedback, practiceKey, sensitiveQuestions } from './coachPractice.js';
const feedbackQuestion = {id:'feedback-q',question:'像素越高就一定越好吗？',evidence:'像素会影响细节和裁切空间，但不能单独决定画质。',source:'参数卡/像素.md'};
const parseRichFeedback = parseFeedback as unknown as (raw:string, question:typeof feedbackQuestion)=>typeof richFeedback;
const richFeedback = {
  score:80,
  verdict:'部分正确',
  correctPoints:['知道像素会影响细节'],
  incorrectPoints:[],
  missingPoints:['没有说明画质还受其他因素影响'],
  keyImprovement:'补上像素不是画质唯一标准。',
  liveImpact:'避免顾客只按像素高低选机。',
  reference:'像素会影响细节和裁切空间，但不能只凭像素判断画质。',
  followUp:'除了像素，选相机还要了解顾客哪些需求？',
  evidenceQuote:'像素会影响细节和裁切空间，但不能单独决定画质。'
};
assert.deepEqual(knowledgeQuestions([]), []);
assert.equal(knowledgeQuestions([{cardId:'K1',title:'卡口',body:'卡口需要匹配',relativePath:'参数/卡口.md'}])[0].source, '参数/卡口.md');
assert.notEqual(practiceKey('luo'), practiceKey('yu'));
assert.throws(() => parseRichFeedback('{"score":101}', feedbackQuestion));
assert.throws(() => parseRichFeedback('{"score":80,"feedback":"","reference":""}', feedbackQuestion));
assert.equal(parseRichFeedback(JSON.stringify(richFeedback), feedbackQuestion).score,80);
assert.throws(() => parseRichFeedback(JSON.stringify({...richFeedback,evidenceQuote:'参数卡里没有的说法'}), feedbackQuestion), '证据摘录必须来自当前题目依据');
assert.throws(() => parseRichFeedback(JSON.stringify({...richFeedback,verdict:'优秀'}), feedbackQuestion), '分析结论必须使用受控枚举');
assert.throws(() => parseRichFeedback(JSON.stringify({...richFeedback,score:95,verdict:'部分正确'}), feedbackQuestion), '分数与分析结论必须一致');
const insufficient = parseRichFeedback(JSON.stringify({...richFeedback,score:null,verdict:'依据不足',correctPoints:[],incorrectPoints:[],missingPoints:[]}), feedbackQuestion);
assert.equal(insufficient.score,null,'依据不足时本题不得用零分惩罚主播');
assert.ok(sensitiveQuestions.length >= 8);
assert.ok(sensitiveQuestions.every(q => q.evidence && q.source && q.question));
console.log('coach practice tests passed');
const api = await import('./coachPractice.js');
const popularCameraQuestions = (api as unknown as { popularCameraQuestions?: unknown[] }).popularCameraQuestions;
assert.ok(Array.isArray(popularCameraQuestions), '应提供常卖机型固定题库');
assert.equal(popularCameraQuestions.length, 60, '题库应包含36道基础题和24道评论情景题');
assert.equal(popularCameraQuestions.filter(q => String((q as { id?: string }).id).startsWith('camera-')).length, 36);
assert.equal(popularCameraQuestions.filter(q => String((q as { id?: string }).id).startsWith('comment-')).length, 24);
const typedQuestions = popularCameraQuestions as Array<{id:string;kind:string;question:string;evidence:string;source:string;options?:string[];correctAnswer?:string}>;
assert.deepEqual({choice:typedQuestions.filter(q=>q.kind==='choice').length,boolean:typedQuestions.filter(q=>q.kind==='boolean').length,open:typedQuestions.filter(q=>q.kind==='open').length},{choice:12,boolean:12,open:36});
assert.ok(typedQuestions.every(q=>q.question && q.evidence && q.source));
assert.ok(typedQuestions.every(q=>!JSON.stringify(q).includes('待确认')));
assert.ok(typedQuestions.filter(q=>q.id.startsWith('comment-')).every(q=>q.source.includes('已去除昵称与账号')));
assert.ok(typedQuestions.every(q=>!/user_id|user_name|房间号|room_id/i.test(JSON.stringify(q))));
assert.equal(api.popularCameraQuestionsFor('choice').length,12);
assert.equal(api.popularCameraQuestionsFor('boolean').length,12);
assert.equal(api.popularCameraQuestionsFor('open').length,36);
assert.equal(api.popularCameraQuestionsFor('mixed').length,60);
const prompt = api.practicePrompt(sensitiveQuestions[0]);
assert.match(prompt, /二手相机/);
assert.match(prompt, /当前这台/);
assert.match(prompt, /新品包装清单/);
assert.match(prompt, /score 必须是 JSON 数字或 null/);
assert.doesNotMatch(prompt, /"score":"/,'评分格式示例不得把数字错误示范成字符串');
assert.equal(typeof api.parseObjectiveQuestion, 'function', '应能校验模型生成的客观题');
const base = knowledgeQuestions([{cardId:'K',title:'卡口',body:'机身与镜头卡口需要匹配。',relativePath:'卡口.md'}])[0];
const raw = {question:'购买前需要匹配什么？',options:['卡口','颜色','重量','品牌字数'],correctAnswer:'A',quote:'卡口需要匹配',explanation:'机身与镜头卡口需要匹配。'};
const q = api.parseObjectiveQuestion(JSON.stringify(raw),base,'choice');
assert.equal(api.objectiveScore(q,'A'),100);
assert.equal(api.objectiveScore(q,'B'),0);
assert.throws(()=>api.parseObjectiveQuestion(JSON.stringify({...raw,quote:'全画幅'}),base,'choice'));
assert.throws(()=>api.parseObjectiveQuestion(JSON.stringify({...raw,options:['卡口','卡口','重量','字数']}),base,'choice'));
assert.throws(()=>api.parseObjectiveQuestion(JSON.stringify({...raw,correctAnswer:'Z'}),base,'choice'));
assert.throws(()=>parseRichFeedback(JSON.stringify({...richFeedback,keyImprovement:'待确认'}), feedbackQuestion));
const analysisApi = api as unknown as {applyObjectiveFeedback?: (question:typeof q,answer:string,feedback:typeof richFeedback)=>typeof richFeedback};
assert.equal(typeof analysisApi.applyObjectiveFeedback, 'function', '客观题判定必须由固定答案覆盖模型意见');
const misleadingModelFeedback = {...richFeedback,score:100,verdict:'正确',incorrectPoints:[],missingPoints:[]};
const fixedWrong = analysisApi.applyObjectiveFeedback!(q,'B',misleadingModelFeedback);
assert.equal(fixedWrong.score,0);
assert.equal(fixedWrong.verdict,'错误');
assert.match(fixedWrong.keyImprovement,/正确答案/);
const fixedCorrect = analysisApi.applyObjectiveFeedback!(q,'A',{...richFeedback,score:0,verdict:'错误'});
assert.equal(fixedCorrect.score,100);
assert.equal(fixedCorrect.verdict,'正确');
const judgment = api.parseObjectiveQuestion(JSON.stringify({...raw,options:['正确','错误']}),base,'boolean');
assert.equal(api.objectiveScore(judgment,'B'),0);
assert.throws(()=>api.parseObjectiveQuestion(JSON.stringify({...raw,options:['是','否']}),base,'boolean'));
assert.throws(()=>api.parseObjectiveQuestion('null',base,'choice'));
assert.equal(api.objectiveScore(base,'自由回答'),null);

const beginnerLessons = (api as unknown as { beginnerCameraLessons?: Array<{
  id:string; model:string; image:string; facts:Array<{label:string;value:string}>;
  hotspot:{label:string;x:number;y:number;radius:number}; partsSourceUrl?:string;
  knowledgePoints:Array<{title:string;plainMeaning:string}>;
  painPoints:Array<{customerNeed:string;concern:string;responseFocus:string}>;
  sellingPoints:Array<{title:string;parameterBasis:string;customerBenefit:string}>;
  boundaries:string[]; livePitch:string; officialSourceUrl:string;
}> }).beginnerCameraLessons;
assert.ok(Array.isArray(beginnerLessons), '应提供零基础看图认机课程');
assert.equal(beginnerLessons.length, 12, '应覆盖已确认的十二款常卖机型');
assert.ok(beginnerLessons.every(lesson => lesson.image.startsWith('/training/camera-cards/')), '课程必须使用本地参数卡原图');
assert.ok(beginnerLessons.every(lesson => ['焦段','有无闪光灯','像素','适用人群'].every(label => lesson.facts.some(fact => fact.label === label))), '每款机型只讲主播需要的四类基础信息');
assert.ok(beginnerLessons.every(lesson => !/套餐|价格|待确认|AI生成/.test(JSON.stringify(lesson))), '认机课程不得混入套餐、价格、占位词或AI机身图');
assert.ok(beginnerLessons.every(lesson => lesson.knowledgePoints.length >= 2), '每款机型至少提供两个小白知识点');
assert.ok(beginnerLessons.every(lesson => lesson.painPoints.length >= 2), '每款机型至少覆盖两个顾客痛点');
assert.ok(beginnerLessons.every(lesson => lesson.sellingPoints.length >= 2), '每款机型至少提供两个有参数依据的卖点');
assert.ok(beginnerLessons.every(lesson => lesson.sellingPoints.every(point => point.parameterBasis && point.customerBenefit)), '卖点必须同时说明参数依据和顾客收益');
assert.ok(beginnerLessons.every(lesson => lesson.boundaries.length >= 1 && lesson.livePitch), '每款机型必须说明使用边界并给出主播说法');
assert.ok(beginnerLessons.every(lesson => /^https:\/\//.test(lesson.officialSourceUrl)), '每款机型必须关联品牌官方来源');
assert.ok(beginnerLessons.every(lesson => !/天花板|封神|同级别无对手|绝对|一定适合|无需升级|爆款素材|性能拉满/.test(JSON.stringify(lesson))), '培训资料不得沿用夸张或绝对化销售词');
assert.deepEqual(new Set(beginnerLessons.map(lesson => lesson.id)), new Set(['canon-r50','canon-r6ii','canon-r8','canon-r7','canon-r10','fuji-xt5','fuji-xs20','fuji-xt30ii','fuji-xt30iii','sony-a7m3','sony-a7m4','sony-a7c2']));
const r50Lesson = beginnerLessons.find(lesson => lesson.id === 'canon-r50');
assert.ok(r50Lesson?.partsSourceUrl?.startsWith('https://cam.start.canon/'), 'R50应关联佳能官方部件图来源');
assert.equal(r50Lesson?.hotspot.label, '快门按钮');
assert.equal(api.evaluateHotspot({x:r50Lesson!.hotspot.x,y:r50Lesson!.hotspot.y}, r50Lesson!.hotspot), true, '点击快门中心应判定正确');
assert.equal(api.evaluateHotspot({x:72,y:37}, r50Lesson!.hotspot), false, 'R50辅助灯位置不得误判为快门按钮');
assert.equal(api.evaluateHotspot({x:99,y:99}, r50Lesson!.hotspot), false, '明显偏离快门按钮应判定错误');
const sonyLesson = beginnerLessons.find(lesson => lesson.id === 'sony-a7m3');
assert.equal(api.evaluateHotspot({x:22.5,y:33}, sonyLesson!.hotspot), true, 'A7M3手柄顶部的快门按钮应判定正确');
assert.equal(api.evaluateHotspot({x:31,y:35}, sonyLesson!.hotspot), false, 'A7M3对焦辅助灯位置不得误判为快门按钮');
const r6Lesson = beginnerLessons.find(lesson => lesson.id === 'canon-r6ii');
assert.equal(api.evaluateHotspot({x:29.5,y:44}, r6Lesson!.hotspot), true, 'R6二代手柄顶部的快门按钮应判定正确');
assert.equal(api.evaluateHotspot({x:42.5,y:41}, r6Lesson!.hotspot), false, 'R6二代机身前方辅助灯不得误判为快门按钮');
const xt5Lesson = beginnerLessons.find(lesson => lesson.id === 'fuji-xt5');
assert.equal(api.evaluateHotspot({x:31.5,y:31.5}, xt5Lesson!.hotspot), true, 'X-T5电源环中央的快门按钮应判定正确');
assert.equal(api.evaluateHotspot({x:41,y:34}, xt5Lesson!.hotspot), false, 'X-T5辅助灯不得误判为快门按钮');
assert.match(JSON.stringify(xt5Lesson), /三向折叠/,'X-T5屏幕必须按官方规格校正');
assert.match(JSON.stringify(xt5Lesson), /机械快门最高约15张\/秒/,'X-T5机械连拍必须按官方规格校正');
assert.doesNotMatch(JSON.stringify(xt5Lesson), /180°|机械快门最高约20张\/秒/,'不得保留X-T5原卡中的错误参数');
const xs20Lesson = beginnerLessons.find(lesson => lesson.id === 'fuji-xs20');
assert.doesNotMatch(JSON.stringify(xs20Lesson), /双卡槽/,'X-S20不得沿用原卡中的双卡槽错误');
const xt30iiiLesson = beginnerLessons.find(lesson => lesson.id === 'fuji-xt30iii');
assert.match(JSON.stringify(xt30iiiLesson), /约378克/,'X-T30 III重量必须使用含电池和存储卡的官方口径');

const companyQuestions = (api as unknown as { companyFoundationQuestions?: Array<{
  id:string; kind:string; question:string; evidence:string; source:string;
  media?:{image:string;alt:string;caption:string};
}> }).companyFoundationQuestions;
assert.ok(Array.isArray(companyQuestions), '应提供校对后的公司基础知识题库');
assert.equal(companyQuestions.length, 12, '公司基础课首版应包含12道高质量样题');
assert.deepEqual({choice:companyQuestions.filter(q=>q.kind==='choice').length,boolean:companyQuestions.filter(q=>q.kind==='boolean').length,open:companyQuestions.filter(q=>q.kind==='open').length},{choice:4,boolean:4,open:4});
assert.ok(companyQuestions.every(q=>q.source.includes('公司内部培训资料（校对版）')));
assert.ok(companyQuestions.every(q=>q.media?.image.startsWith('/training/company-foundation/') && q.media.alt && q.media.caption), '每道公司基础题都必须配有可见图片');
assert.ok(companyQuestions.every(q=>!/中画幅相机；A6|一定优于|防抖功能将会失效|待确认/.test(JSON.stringify(q))), '不得把原资料中的错误或绝对化表述带进题库');
