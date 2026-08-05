from pathlib import Path

from docx import Document
from docx.oxml.ns import qn
from docx.shared import Inches, Pt, RGBColor

import build_yuqianhui_training_doc as base


OUTPUT = Path(
    r"C:\Users\10230\Documents\Codex\Workspace\01-Projects\Project-003-直播切片分析系统"
    r"\06-交付物\金典拍拍二手影像直播通用新人培训手册_2026-07-24.docx"
)


def configure_document(doc):
    section = doc.sections[0]
    section.top_margin = Inches(1)
    section.bottom_margin = Inches(1)
    section.left_margin = Inches(1)
    section.right_margin = Inches(1)
    section.header_distance = Inches(0.492)
    section.footer_distance = Inches(0.492)
    footer = section.footer
    footer_p = footer.paragraphs[0]
    footer_p.alignment = 2
    footer_p.paragraph_format.space_before = Pt(0)
    footer_p.paragraph_format.space_after = Pt(0)
    base.add_text(footer_p, "金典拍拍通用新人培训手册 | 内部培训使用 | ", size=8.5, color=base.MUTED)
    field = base.OxmlElement("w:fldSimple")
    field.set(qn("w:instr"), "PAGE")
    footer_p._p.append(field)

    normal = doc.styles["Normal"]
    normal.font.name = "Microsoft YaHei"
    normal._element.rPr.rFonts.set(qn("w:eastAsia"), "Microsoft YaHei")
    normal.font.size = Pt(10.5)
    normal.paragraph_format.space_after = Pt(6)
    normal.paragraph_format.line_spacing = 1.18
    for level, size, color in ((1, 16, base.BLUE), (2, 13, base.BLUE), (3, 11.5, base.DARK_BLUE)):
        style = doc.styles[f"Heading {level}"]
        style.font.name = "Microsoft YaHei"
        style._element.rPr.rFonts.set(qn("w:eastAsia"), "Microsoft YaHei")
        style.font.size = Pt(size)
        style.font.color.rgb = RGBColor.from_string(color)
        style.font.bold = True
        style.paragraph_format.space_before = Pt({1: 16, 2: 12, 3: 8}[level])
        style.paragraph_format.space_after = Pt({1: 8, 2: 6, 3: 4}[level])


def add_metadata(doc):
    rows = [
        ("培训对象", "金典拍拍二手影像直播间新主播、场控与复盘人员"),
        ("综合样本", "罗雨欣 7 场 + 于千惠 6 场直播整理"),
        ("母稿依据", "MS-BATCH-1 V1.1 与 MS-BATCH-2 V1.0，均已审核发布"),
        ("培训目标", "统一成交结构、话术边界与新人开播前检查动作"),
        ("使用原则", "学习结构，不复用历史价格、库存、赠品、链接或未经确认的型号"),
    ]
    table = doc.add_table(rows=len(rows), cols=2)
    base.set_table_geometry(table, [Inches(1.35), Inches(5.15)])
    for row, (label, value) in zip(table.rows, rows):
        base.shade(row.cells[0], base.LIGHT_GRAY)
        for cell in row.cells:
            p = cell.paragraphs[0]
            p.paragraph_format.space_after = Pt(0)
        base.add_text(row.cells[0].paragraphs[0], label, bold=True, color=base.DARK_BLUE)
        base.add_text(row.cells[1].paragraphs[0], value)
    doc.add_paragraph().paragraph_format.space_after = Pt(2)


def add_nine_step_table(doc):
    headers = ["环节", "目标", "新人可用表达", "必须确认"]
    rows = [
        ("1. 开场欢迎", "让新进用户知道直播间卖什么、可以怎么问。", "欢迎老师们光临金典拍拍，想看的型号和预算可以直接打在公屏上。", "当前品类、活动与领券入口"),
        ("2. 需求确认", "明确预算、用途、卡口和偏好。", "老师您主要拍什么？预算大概多少？配什么卡口？我按需求给您找。", "需求、预算、卡口、用途"),
        ("3. 实物展示", "让用户看清真实状态与差异。", "这只就是现在给您看的实物，我按外观、关键部位和配件给您看一遍。", "型号、成色、附件、瑕疵"),
        ("4. 功能说明", "讲清已确认功能与适用场景。", "卡口、镜片/CMOS、光圈和对焦，我按实物和检测结果给您讲。", "检测结果、使用限制"),
        ("5. 链接与报价", "让用户找到正确商品并理解到手价。", "小黄车X号链接，优惠按当前页面来；我和您确认型号、成色和到手价。", "链接、价格、优惠、规格"),
        ("6. 异议处理", "回应价格、成色、售后或库存顾虑。", "您担心的是【问题】。我们能确认的是【事实/规则】，不清楚的我现在核实。", "售后规则、库存、可承诺范围"),
        ("7. 成交与发货", "明确拍下规格、备注和下一步。", "您拍XX规格，拍完和我说一声；发货和售后以当场页面、实物和客服确认为准。", "发货时效、赠品、备注规则"),
        ("8. 回收置换", "承接换购、回收和后续沟通。", "回收或换购的规则我帮您对接负责人确认，先把具体型号和状态发我。", "回收规则、负责人、渠道"),
        ("9. 转场互动", "商品切换时维持停留和需求收集。", "新进来的老师，把想看的型号和预算打在公屏，我继续给您做成色展示。", "当前可讲品类与活动"),
    ]
    table = doc.add_table(rows=1, cols=4)
    base.set_table_geometry(table, [Inches(0.9), Inches(1.25), Inches(2.95), Inches(1.4)])
    base.set_repeat_table_header(table.rows[0])
    for cell, header in zip(table.rows[0].cells, headers):
        base.shade(cell, base.DARK_BLUE)
        p = cell.paragraphs[0]
        p.paragraph_format.space_after = Pt(0)
        base.add_text(p, header, size=9.3, color=base.WHITE, bold=True)
    for values in rows:
        cells = table.add_row().cells
        for cell, value in zip(cells, values):
            p = cell.paragraphs[0]
            p.paragraph_format.space_after = Pt(0)
            p.paragraph_format.line_spacing = 1.06
            base.add_text(p, value, size=8.55)
    doc.add_paragraph().paragraph_format.space_after = Pt(2)


def add_style_table(doc):
    headers = ["来源", "最值得学习的特点", "新人如何使用"]
    rows = [
        ("罗雨欣", "完整覆盖开场、需求、展示、报价、异议、成交、回收与转场九类场景。", "适合学习整场直播的节奏：先问后推、每个环节都给用户一个明确下一步。"),
        ("于千惠", "需求型导购更集中：先问预算/卡口，再用实物、成色透明和售后规则减少顾虑。", "适合学习单品成交：实物讲清楚、价格说完整、售后有边界。"),
        ("共同标准", "不猜测型号，不复制历史价格；所有优势必须落到可核验的服务事实。", "任何主播都按当前商品页、仓库、实物和客服政策确认后再说。"),
    ]
    table = doc.add_table(rows=1, cols=3)
    base.set_table_geometry(table, [Inches(1.1), Inches(2.75), Inches(2.95)])
    base.set_repeat_table_header(table.rows[0])
    for cell, header in zip(table.rows[0].cells, headers):
        base.shade(cell, base.DARK_BLUE)
        p = cell.paragraphs[0]
        p.paragraph_format.space_after = Pt(0)
        base.add_text(p, header, size=9.4, color=base.WHITE, bold=True)
    for values in rows:
        cells = table.add_row().cells
        for cell, value in zip(cells, values):
            p = cell.paragraphs[0]
            p.paragraph_format.space_after = Pt(0)
            p.paragraph_format.line_spacing = 1.1
            base.add_text(p, value, size=9.2)
    doc.add_paragraph().paragraph_format.space_after = Pt(2)


def add_common_scripts(doc):
    scripts = [
        ("欢迎与互动", "欢迎老师们光临金典拍拍，想看什么型号、预算大概多少，直接打在公屏，我按需求给您找。"),
        ("需求确认", "老师您主要拍什么题材？预算大概多少？配什么卡口？您更看重轻便、虚化还是焦段，我按这个给您推荐。"),
        ("实物讲品", "这只就是您现在看到的实物。我先看外观和边角，再看卡口、镜片/CMOS、光圈和对焦；哪里有使用痕迹我提前讲清楚。"),
        ("链接与价格", "我给您放到小黄车X号链接。价格和优惠按当前页面来，您拍下前我再和您确认一次对应型号、成色和到手价。"),
        ("异议处理", "您担心的是【问题】。我们目前能确认的是【实物状态/政策】；其他信息我先核实后再给您准确回复。"),
        ("成交与售后", "您拍XX规格，拍完和我说一声，我按当场规则给您备注。到手后先试机，物流和售后以当前页面、客服和订单信息为准。"),
    ]
    for title, script in scripts:
        p = doc.add_paragraph()
        p.paragraph_format.space_before = Pt(4)
        p.paragraph_format.space_after = Pt(2)
        base.add_text(p, f"{title}：", color=base.DARK_BLUE, bold=True)
        p = doc.add_paragraph()
        p.paragraph_format.left_indent = Inches(0.18)
        p.paragraph_format.right_indent = Inches(0.12)
        p.paragraph_format.space_after = Pt(6)
        p.paragraph_format.line_spacing = 1.18
        base.add_text(p, f"“{script}”")


def add_advantage_table(doc):
    headers = ["可说明的优势", "新人说法", "禁止的说法"]
    rows = [
        ("实物一致与成色透明", "直播间按实物展示，外观和关键部位提前讲清；已确认的功能按检测和实物说明。", "“绝对没有瑕疵”“什么功能都没问题”"),
        ("售后可追溯", "下单后的物流和售后以当前页面、客服和订单信息为准，有问题可通过订单继续处理。", "“任何情况都能退”“所有商品都完全一样的售后”"),
        ("直播间服务", "先按预算和需求找货，优惠、赠品和库存以当场确认的信息为准。", "“全网最低”“永远有货”“赠品一定有”"),
    ]
    table = doc.add_table(rows=1, cols=3)
    base.set_table_geometry(table, [Inches(1.5), Inches(3.0), Inches(2.3)])
    base.set_repeat_table_header(table.rows[0])
    for cell, header in zip(table.rows[0].cells, headers):
        base.shade(cell, base.DARK_BLUE)
        p = cell.paragraphs[0]
        p.paragraph_format.space_after = Pt(0)
        base.add_text(p, header, size=9.4, color=base.WHITE, bold=True)
    for values in rows:
        cells = table.add_row().cells
        for cell, value in zip(cells, values):
            p = cell.paragraphs[0]
            p.paragraph_format.space_after = Pt(0)
            p.paragraph_format.line_spacing = 1.1
            base.add_text(p, value, size=9.0)
    doc.add_paragraph().paragraph_format.space_after = Pt(2)


def main():
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    doc = Document()
    configure_document(doc)

    p = doc.add_paragraph()
    p.paragraph_format.space_after = Pt(3)
    base.add_text(p, "新人培训手册", size=11, color=base.BLUE, bold=True)
    p = doc.add_paragraph()
    p.paragraph_format.space_after = Pt(4)
    base.add_text(p, "金典拍拍二手影像直播通用培训手册", size=23, color=base.INK, bold=True)
    base.set_paragraph_border(p)
    p = doc.add_paragraph()
    p.paragraph_format.space_after = Pt(16)
    base.add_text(p, "整合罗雨欣与于千惠已发布母稿，供新人开播、讲品与复盘使用", size=12, color=base.MUTED)

    base.add_callout(
        doc,
        "培训结论",
        "两位主播的话术共同证明：二手影像直播的关键不是死记原话，而是先确认需求，再展示实物，讲清价格与售后边界，并给用户明确的下一步。新人只可复用结构和表达方式，价格、库存、赠品、型号和链接必须以当场事实为准。",
    )
    add_metadata(doc)

    base.add_heading(doc, "一、共同的成交原则")
    for item in [
        "先问需求，再给建议：预算、用途、卡口和偏好没确认前，不直接推具体商品。",
        "先讲实物，再报价格：成色、瑕疵、关键部位与功能状态应按实物或检测结果说明。",
        "链接、优惠和到手价要成套说清：用户应知道买哪一件、在哪个链接拍、最终多少钱。",
        "所有承诺都要有依据：库存、发货、赠品、退换、保修和优惠以当场商品页、仓库、客服或负责人确认的信息为准。",
        "不确定就核实：宁可说“我确认后回复您”，也不能用猜测补全型号、价格或售后。",
    ]:
        base.add_bullet(doc, item)

    base.add_heading(doc, "二、统一的九步直播结构")
    add_nine_step_table(doc)

    base.add_heading(doc, "三、两位主播的可学习差异")
    add_style_table(doc)

    base.add_heading(doc, "四、可直接用于直播的标准表达")
    add_common_scripts(doc)

    base.add_heading(doc, "五、如何说明“为什么选择我们”")
    base.add_body(doc, "不要贬低其他商家，也不要用无法证明的“最低价”“最好”成交。把优势讲成用户能验证的服务事实：实物一致、成色透明、售后可追溯和需求响应。")
    add_advantage_table(doc)
    base.add_callout(
        doc,
        "统一优势说明（确认后使用）",
        "老师，二手买的最重要就是实物、成色和售后。直播间给您看的就是按这只实物来安排，外观和关键部位我都提前讲清楚。价格、库存、物流和售后按当前商品页面、实物和客服确认的信息来；您下单后有问题，也可以从订单直接找到我们继续处理。",
        fill=base.CAUTION,
    )

    base.add_heading(doc, "六、新人开播前与下播后检查")
    checks = [
        "开播前确认：当前商品型号、成色、价格、优惠、库存、链接号、赠品和可发货时间。",
        "讲品前确认：实物外观、附件、关键部位和已完成检测的功能状态。",
        "下单前确认：用户拍下的具体规格、链接、备注动作以及当场有效的福利。",
        "售后前确认：退换、保修、运费险和物流规则是否适用于当前商品。",
        "下播后复盘：只把原话、事实、适用场景和风险边界都通过审核的片段，纳入下一版母稿。",
    ]
    for item in checks:
        base.add_number(doc, item)

    base.add_callout(
        doc,
        "使用边界",
        "本手册汇总罗雨欣 MS-BATCH-1 V1.1（7 场样本）与于千惠 MS-BATCH-2 V1.0（6 场样本）的已发布母稿。它用于统一新人表达与复盘方法，不替代商品页、客服政策、仓库库存、负责人确认或真实经营数据。",
        fill=base.LIGHT_GRAY,
    )

    doc.core_properties.title = "金典拍拍二手影像直播通用新人培训手册"
    doc.core_properties.subject = "新人直播培训"
    doc.core_properties.author = "典典直播切片"
    doc.save(OUTPUT)
    print(OUTPUT)


if __name__ == "__main__":
    main()
