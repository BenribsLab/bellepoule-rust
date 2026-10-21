package fr.bellepoule.smartpoule

import android.content.Context
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView

/** Petits outils pour construire les écrans par le code. */
object Ui {
    const val BLUE = 0xFF1F5AE0.toInt()
    const val INK = 0xFF15294F.toInt()
    const val MUTED = 0xFF5F6F8C.toInt()
    const val GREEN = 0xFF168456.toInt()
    const val RED = 0xFFB42318.toInt()
    const val CARD = 0xFFF3F6FB.toInt()

    fun dp(context: Context, v: Int): Int =
        TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v.toFloat(), context.resources.displayMetrics).toInt()

    fun column(context: Context, padding: Int = 16): LinearLayout = LinearLayout(context).apply {
        orientation = LinearLayout.VERTICAL
        val p = dp(context, padding)
        setPadding(p, p, p, p)
    }

    fun row(context: Context): LinearLayout = LinearLayout(context).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
    }

    fun text(context: Context, value: String, size: Float = 16f, bold: Boolean = false, color: Int = INK): TextView =
        TextView(context).apply {
            text = value
            textSize = size
            setTextColor(color)
            if (bold) setTypeface(typeface, Typeface.BOLD)
        }

    fun button(context: Context, label: String, primary: Boolean = true, onClick: () -> Unit): Button =
        Button(context).apply {
            text = label
            isAllCaps = false
            textSize = 16f
            setTextColor(if (primary) Color.WHITE else BLUE)
            background = rounded(if (primary) BLUE else Color.WHITE, if (primary) BLUE else BLUE, 10f, context)
            setOnClickListener { onClick() }
        }

    fun rounded(fill: Int, stroke: Int, radius: Float, context: Context): GradientDrawable = GradientDrawable().apply {
        setColor(fill)
        cornerRadius = dp(context, radius.toInt()).toFloat()
        setStroke(dp(context, 1), stroke)
    }

    /** Carte cliquable (liste des matchs). */
    fun card(context: Context): LinearLayout = column(context, 12).apply {
        background = rounded(CARD, 0xFFDCE5F4.toInt(), 12f, context)
        isClickable = true
        isFocusable = true
        val lp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        lp.bottomMargin = dp(context, 10)
        layoutParams = lp
    }

    fun space(context: Context, height: Int): View = View(context).apply {
        layoutParams = LinearLayout.LayoutParams(1, dp(context, height))
    }
}
