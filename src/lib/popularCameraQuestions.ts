import type { PracticeQuestion, QuestionKind } from './coachPractice.js';

type CameraFact = {
  id: string; name: string; aliases: string; pixels: string; sensor: string;
  hasFlash: boolean; audience: string; source: string;
};

const cameras: CameraFact[] = [
  {id:'canon-r6ii',name:'佳能 EOS R6 Mark II',aliases:'R6二代、R6 II、R62',pixels:'约2420万',sensor:'全画幅',hasFlash:false,audience:'想兼顾照片和视频、拍运动题材或从半画幅升级的用户',source:'佳能官方：https://www.canon.com.cn/product/r6ii/'},
  {id:'canon-r50',name:'佳能 EOS R50',aliases:'R50',pixels:'约2420万',sensor:'APS-C画幅',hasFlash:true,audience:'刚入门、拍日常、旅行和短视频的用户',source:'佳能官方：https://www.canon.com.cn/product/r50/spec.html'},
  {id:'canon-r8',name:'佳能 EOS R8',aliases:'R8',pixels:'约2420万',sensor:'全画幅',hasFlash:false,audience:'想要轻便全画幅，主要拍旅行、人像并兼顾视频的用户',source:'佳能官方：https://www.canon.com.cn/product/r8/index.html'},
  {id:'canon-r7',name:'佳能 EOS R7',aliases:'R7',pixels:'约3250万',sensor:'APS-C画幅',hasFlash:false,audience:'常拍运动、宠物、鸟类等远处或快速主体的用户',source:'佳能官方：https://www.canon.com.cn/product/r7/spec.html'},
  {id:'canon-r10',name:'佳能 EOS R10',aliases:'R10',pixels:'约2420万',sensor:'APS-C画幅',hasFlash:true,audience:'希望轻便入门，又想练习抓拍和手动操作的用户',source:'佳能官方：https://www.canon.com.cn/product/r10/spec.html'},
  {id:'fuji-xt5',name:'富士 X-T5',aliases:'XT5、X-T5',pixels:'约4020万',sensor:'APS-C画幅',hasFlash:false,audience:'重视照片细节、直出风格、旅行和街拍的用户',source:'富士官方：https://www.fujifilm-x.com/zh-cn/products/cameras/x-t5/specifications/'},
  {id:'fuji-xs20',name:'富士 X-S20',aliases:'XS20、X-S20',pixels:'约2610万',sensor:'APS-C画幅',hasFlash:true,audience:'希望一台轻便机身兼顾照片、视频和Vlog的用户',source:'富士官方：https://www.fujifilm-x.com/zh-cn/products/cameras/x-s20/specifications/'},
  {id:'fuji-xt30ii',name:'富士 X-T30 II',aliases:'XT30二代、X-T30 II',pixels:'约2610万',sensor:'APS-C画幅',hasFlash:true,audience:'喜欢轻便复古机身，拍日常、旅行和街拍的入门用户',source:'富士官方：https://www.fujifilm-x.com/zh-cn/products/cameras/x-t30-ii/'},
  {id:'fuji-xt30iii',name:'富士 X-T30 III',aliases:'XT30三代、X-T30 III',pixels:'约2610万',sensor:'APS-C画幅',hasFlash:true,audience:'喜欢轻便复古外形、胶片模拟和日常随拍的用户',source:'富士官方：https://www.fujifilm-x.com/zh-cn/products/cameras/x-t30-iii/'},
  {id:'sony-a7m3',name:'索尼 α7 III',aliases:'A7M3、7M3、α7 III',pixels:'约2420万',sensor:'全画幅',hasFlash:false,audience:'想入门全画幅并兼顾照片和视频的用户',source:'索尼官方：https://www.sony.com.cn/zh-cn/cms/newscenter/product/2018/180227-1.html'},
  {id:'sony-a7m4',name:'索尼 α7 IV',aliases:'A7M4、7M4、α7 IV',pixels:'约3300万',sensor:'全画幅',hasFlash:false,audience:'对画质、对焦和照片视频双修都有较高要求的用户',source:'索尼官方：https://www.sony.com.cn/content/sonyportal/zh-cn/cms/newscenter/product/2021/20211022-1.html.html'},
  {id:'sony-a7c2',name:'索尼 α7C II',aliases:'A7C2、7C2、α7C II',pixels:'约3300万',sensor:'全画幅',hasFlash:false,audience:'重视轻便，常拍旅行、美食和日常记录的用户',source:'索尼官方：https://www.sony.com.cn/content/sonyportal/zh-cn/cms/newscenter/product/2023/20230830-1.html.html'},
];

const pixelPool = ['约2420万','约2610万','约3250万','约3300万','约4020万'];

function cameraQuestions(camera: CameraFact, index: number): PracticeQuestion[] {
  const options = pixelPool.filter(value => value !== camera.pixels).slice(0, 3);
  const correctIndex = index % 4;
  options.splice(correctIndex, 0, camera.pixels);
  const fact = `${camera.name}（常用叫法：${camera.aliases}）为${camera.sensor}可换镜头相机，有效像素${camera.pixels}，机身${camera.hasFlash ? '带有' : '没有'}内置闪光灯。可换镜头机身没有固定焦段，焦段由所配镜头决定。较适合${camera.audience}。`;
  const truth = index % 2 === 0;
  const statedFlash = truth ? camera.hasFlash : !camera.hasFlash;
  return [
    {id:`camera-${camera.id}-choice`,kind:'choice',question:`${camera.name}的有效像素大约是多少？`,options,correctAnswer:String.fromCharCode(65+correctIndex),explanation:`${camera.name}的有效像素为${camera.pixels}。`,evidence:fact,source:camera.source},
    {id:`camera-${camera.id}-boolean`,kind:'boolean',question:`判断：${camera.name}机身${statedFlash?'带有':'没有'}内置闪光灯。`,options:['正确','错误'],correctAnswer:truth?'A':'B',explanation:`${camera.name}机身${camera.hasFlash?'带有':'没有'}内置闪光灯。`,evidence:fact,source:camera.source},
    {id:`camera-${camera.id}-open`,kind:'open',question:`观众问：“${camera.aliases.split('、')[0]}适合我这种新手吗？”请先追问用途，再用容易听懂的话介绍它适合什么人。`,evidence:fact,source:camera.source},
  ];
}

const commentSource = '真实直播评论原型（已去除昵称与账号并改写）';
const official = (ids: string) => cameras.filter(camera => ids.split(',').includes(camera.id)).map(camera => camera.source).join('；');
const scenario = (id: number, question: string, evidence: string, modelIds = ''): PracticeQuestion => ({
  id:`comment-${String(id).padStart(2,'0')}`,kind:'open',question:`直播间有人问：“${question}”你怎么回答？`,evidence,
  source:[commentSource, official(modelIds)].filter(Boolean).join('；'),
});

const commentQuestions: PracticeQuestion[] = [
  scenario(1,'X-S20和X-T30三代推荐哪个？','先确认主要拍照片、视频还是Vlog，以及是否在意机身操作方式。两者均为约2610万像素APS-C机型且有内置闪光灯；X-S20更偏照片视频兼顾，X-T30 III更偏轻便复古和日常随拍。','fuji-xs20,fuji-xt30iii'),
  scenario(2,'这台没有翻新吧？','翻新、拆修和历史使用情况属于当前单台二手商品事实。只能依据本场检测记录和商品页面回答；没有证据时说明现阶段能确认的检测结果，不能作绝对保证。'),
  scenario(3,'这是国行的吗？','销售版本属于当前单台商品事实。先核对商品页面、机身标识或可核验凭证，再回答；不能因为同型号通常在国内销售就推断当前这台是国行。'),
  scenario(4,'快门多少，敢保证是真实累计吗？','可说明现场软件读取值，但二手设备历史无法仅凭本次读取完整核验。不得把读取值绝对表述为真实累计，也不要编造具体数字。'),
  scenario(5,'95新和99新到底差在哪？','成色等级必须回到当前平台或店铺的分级标准，并结合当前商品实拍说明外观差异；不能只凭等级名称推断具体划痕、快门或拆修情况。'),
  scenario(6,'配个什么镜头合适？','先问拍摄用途、常拍距离、室内外和预算。机身没有固定焦段，焦段由镜头决定。本轮不介绍套餐，不虚构库存、赠品和当前链接配置。'),
  scenario(7,'R8多少钱，还能便宜吗？','价格必须读取当前直播链接和可用优惠条件。可以先确认预算与用途，再引导查看当前页面；不能沿用旧场次价格或承诺额外优惠。R8为约2420万像素全画幅机型。','canon-r8'),
  scenario(8,'R50和R10，新手选哪个？','两者均为约2420万像素APS-C机型并带内置闪光灯。R50更偏轻便入门、日常和短视频；R10适合希望进一步练习抓拍与手动操作的用户。还需追问照片或视频、常拍题材和预算。','canon-r50,canon-r10'),
  scenario(9,'R7和R10哪个更适合拍运动？','R7约3250万像素APS-C，更适合经常拍运动、宠物、鸟类等远处或快速主体；R10约2420万像素APS-C，定位更轻便入门。还需确认拍摄距离、题材频率和镜头预算。','canon-r7,canon-r10'),
  scenario(10,'R8和R6二代怎么选？','两者均为约2420万像素全画幅机型。R8更强调轻便；R6 Mark II更适合重视综合操控、照片视频双修与运动题材的用户。应继续确认重量、题材、视频需求和预算。','canon-r8,canon-r6ii'),
  scenario(11,'X-T30二代跟三代有什么区别？','两者均为约2610万像素APS-C机型并带内置闪光灯。回答差异时只说已核实资料；再追问用户是否更看重新一代处理器与胶片模拟操作，不能把当前二手机器的成色和配件混为型号差异。','fuji-xt30ii,fuji-xt30iii'),
  scenario(12,'X-T5适合完全新手吗？','X-T5为约4020万像素APS-C机型，重视照片细节、直出风格和拨盘操作。是否适合不能只看“新手”标签，应追问是否愿意学习拨盘操作、主要拍什么和预算。机身没有内置闪光灯。','fuji-xt5'),
  scenario(13,'A7M3和A7M4区别大吗？','两者均为全画幅。A7M3约2420万像素，适合入门全画幅和照片视频兼顾；A7M4约3300万像素，更适合对画质、对焦和双修要求更高的用户。应继续确认预算和拍摄需求。','sony-a7m3,sony-a7m4'),
  scenario(14,'A7M4和A7C2，旅行拍选哪个？','两者均为约3300万像素全画幅。A7C II更强调紧凑轻便与旅行日常；A7 IV更适合重视综合操控和照片视频双修的用户。要追问重量、握持、镜头和视频需求。','sony-a7m4,sony-a7c2'),
  scenario(15,'这个是全画幅吗？','先确认观众指的是哪一个链接或哪台机身，再回答画幅。R6 Mark II、R8、A7M3、A7M4、A7C2为全画幅，其余七款为APS-C画幅。','canon-r6ii,canon-r8,sony-a7m3,sony-a7m4,sony-a7c2'),
  scenario(16,'这台有内置闪光灯吗？','先确认具体型号。R50、R10、X-S20、X-T30 II、X-T30 III带内置闪光灯；其余七款没有内置闪光灯。不要把外接闪光灯当作内置闪光灯。','canon-r50,canon-r10,fuji-xs20,fuji-xt30ii,fuji-xt30iii'),
  scenario(17,'R62就是R6二代吗？','“R62”是直播和用户交流中对佳能 EOS R6 Mark II的简写，也常写作R6 II或R6二代。介绍时先把简称对应到完整型号，避免和R6初代混淆。','canon-r6ii'),
  scenario(18,'A7M4焦段多少？','A7M4是可换镜头机身，本身没有固定焦段；焦段由所配镜头决定。先问观众指机身还是某个镜头或链接，再根据用途继续解释。','sony-a7m4'),
  scenario(19,'女生旅行用，别太重，推荐哪台？','先问照片还是视频、是否接受全画幅镜头重量和预算。可从轻便方向比较R50、R8、X-S20、X-T30系列和A7C II，但最终还要把镜头重量与当前商品情况一起核对。','canon-r50,canon-r8,fuji-xs20,fuji-xt30iii,sony-a7c2'),
  scenario(20,'拍视频和照片都要，推荐哪台？','“都要”仍不够具体。继续问视频比例、是否自拍、常拍题材、预算和对重量的接受度，再从照片视频兼顾机型中缩小范围。','canon-r6ii,canon-r8,fuji-xs20,sony-a7m3,sony-a7m4,sony-a7c2'),
  scenario(21,'理发店拍客人，主要拍人像，推荐哪个型号？','先问店内空间、光线、照片或视频、预算以及是否已有镜头。机身选择之外，镜头视角与光线条件同样重要；不能只听到“人像”就直接报一台，也不能编造当前套餐。'),
  scenario(22,'刚来，完全新手，哪个好？','先用两到三个简单问题确认预算、主要拍什么、是否拍视频和对重量的要求，再给一到两个方向。不要一次堆出全部型号，也不要把“新手”直接等同于最低价机型。'),
  scenario(23,'链接里有原盒、电池和充电器吗？','原盒、配件和数量属于当前单台商品事实，必须读取当前商品页面或现场清单后回答。不能用新品包装清单、同型号其他机器或旧场次记录代替。'),
  scenario(24,'像素多少？有闪光灯吗？新手能用吗？','先确认具体型号，再按“型号确认—像素—内置闪光灯—适用人群—补问用途”的顺序简洁回答。多人或连续问题时先复述核心问题，避免漏答和串台。'),
];

export const popularCameraQuestions: PracticeQuestion[] = cameras.flatMap(cameraQuestions).concat(commentQuestions);

export function popularCameraQuestionsFor(kind: QuestionKind | 'mixed'): PracticeQuestion[] {
  return kind === 'mixed' ? popularCameraQuestions : popularCameraQuestions.filter(question => question.kind === kind);
}
